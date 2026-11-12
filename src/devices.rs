//! Shared device enrollment and authenticated transport to directly paired servers.
use crate::protocol::{DEVICES, Device, Follow, Notifications, Push, SessionId, now};
use axum::http::StatusCode;
use iroh::endpoint::{
    AfterHandshakeOutcome, Connection, EndpointHooks, Side, WeakConnectionHandle,
};
use std::{
    fmt,
    io::ErrorKind,
    path::{Path as FilePath, PathBuf},
    sync::{Arc, Mutex, Weak, atomic::AtomicBool, atomic::Ordering::Relaxed},
    time::Duration,
};
use tokio::{
    fs::OpenOptions,
    io::AsyncWriteExt,
    sync::{RwLock, watch},
};

pub(crate) mod pairing;
mod transport;

pub(crate) const KEY: &str = "device.key";
pub(crate) const HOP: &str = "x-razor-forwarded";
pub(crate) type Gone = watch::Receiver<()>;
const MOST: usize = 64;
/// Eight connections per key allow overlapping relaunches without blocking sibling extensions.
const CONNECTIONS: usize = 8;
/// Why pairing is refused while `provider_only` is on.
pub(crate) const QUIET: &str = "provider_only is on, so Razor neither pairs with nor reaches other Razor servers; set provider_only = false in your settings to pair.";

/// The keys this server accepts. Each device's open streams hold a receiver of that device's
/// channel; removing the device drops the sender, which ends exactly those streams.
pub(crate) struct Devices {
    path: PathBuf,
    client: RwLock<Option<Arc<razor_network::Client>>>,
    local: String,
    /// `provider_only` as the controller last read it: while set no pairing is direct, so nothing
    /// reaches a paired server.
    pub(crate) quiet: AtomicBool,
    /// Wakes the peers loop when the paired set grows: a new pairing, or `provider_only` off.
    pub(crate) grown: tokio::sync::Notify,
    /// Never dropped: the local key's streams end only with the server.
    forever: watch::Sender<()>,
    /// Wakes the push sender after a durable registration or follow change.
    changed: watch::Sender<()>,
    enrolled: RwLock<Vec<(Device, watch::Sender<()>)>>,
    /// Each open session stream's device (None for the local key) and session; the stream holds
    /// the strong half. One entry per open stream at most: ended ones go at the next open.
    streams: Mutex<Vec<Stream>>,
    /// Every connection admission let through, either way, so revocation can close it; as many
    /// as are open, since ended ones go at each handshake.
    connections: Mutex<Vec<WeakConnectionHandle>>,
    /// The one outstanding pairing code, held while a pairing it accepted is saved.
    pairing: tokio::sync::Mutex<Option<pairing::Offer>>,
}

type Stream = (Option<String>, SessionId, Weak<()>);

/// Runs after each handshake, before iroh hands the connection on, so a connection it rejects
/// never reaches a stream.
#[derive(Debug)]
pub(crate) struct Admission(pub(crate) Weak<Devices>);

impl EndpointHooks for Admission {
    async fn after_handshake(&self, connection: &Connection) -> AfterHandshakeOutcome {
        let result = match self.0.upgrade() {
            Some(devices) => devices.admit(connection).await,
            None => Err(1),
        };
        match result {
            Ok(()) => AfterHandshakeOutcome::Accept,
            Err(code) => AfterHandshakeOutcome::Reject {
                error_code: code.into(),
                reason: b"Connection refused".to_vec(),
            },
        }
    }
}

fn public(device: &Device) -> Device {
    Device {
        key: None,
        push: None,
        follows: Vec::new(),
        notifications: None,
        relays: Vec::new(),
        ..device.clone()
    }
}

impl Devices {
    /// Read `device.key`, making it at the first start, and `devices.json`. A device with network
    /// keys answers to them alone: a key it held before pairing by code is dropped here, and from
    /// the file at its next write, as is the address a server was paired at before.
    pub(crate) async fn open(home: &FilePath) -> Result<Devices, Error> {
        let local = kept(&home.join(KEY), 32, "Reading or making device.key").await?;
        let path = home.join(DEVICES);
        let enrolled: Vec<Device> = match tokio::fs::read(&path).await {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| {
                Error::Refused(format!(
                    "devices.json does not parse ({e}); fix it, or remove it and enroll the devices again."
                ))
            })?,
            Err(e) if e.kind() == ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(other("Reading devices.json")(e)),
        };
        let enrolled = enrolled
            .into_iter()
            .map(|mut d| {
                d.key.take_if(|_| !d.keys.is_empty());
                (d, watch::channel(()).0)
            })
            .collect();
        Ok(Devices {
            path,
            client: RwLock::default(),
            local,
            quiet: AtomicBool::new(false),
            grown: tokio::sync::Notify::new(),
            forever: watch::channel(()).0,
            changed: watch::channel(()).0,
            enrolled: RwLock::new(enrolled),
            streams: Mutex::default(),
            connections: Mutex::default(),
            pairing: tokio::sync::Mutex::default(),
        })
    }

    /// Records `provider_only`; turning it off brings back every paired server.
    pub(crate) fn set_quiet(&self, quiet: bool) {
        if self.quiet.swap(quiet, Relaxed) && !quiet {
            self.grown.notify_one();
        }
    }

    /// Tells each open peer connection that the server is stopping; whether there was one, for
    /// the endpoint's close to wait until it has heard.
    pub(crate) fn stop(&self) -> bool {
        let connections = self.connections.lock().unwrap_or_else(|e| e.into_inner());
        let open = connections.iter().filter_map(|c| c.upgrade());
        let open = open.filter(|c| c.close_reason().is_none());
        open.inspect(|c| c.close(1u32.into(), b"Server stopped"))
            .count()
            > 0
    }

    /// Current direct pairings, with the receiver each open stream watches for revocation.
    pub(crate) async fn paired(&self) -> Vec<(Device, Gone)> {
        let enrolled = self.enrolled.read().await;
        enrolled
            .iter()
            .filter(|(d, _)| d.is_peer() && !self.quiet.load(Relaxed))
            .map(|(d, gone)| (d.clone(), gone.subscribe()))
            .collect()
    }

    /// Changes the enrolled device `name`, if it still is, and rewrites `devices.json`.
    pub(crate) async fn change(
        &self,
        name: &str,
        change: impl FnOnce(&mut Device),
    ) -> Result<(), Error> {
        let mut enrolled = self.enrolled.write().await;
        let Some(i) = enrolled.iter().position(|(d, _)| d.name == name) else {
            return Ok(());
        };
        let mut device = enrolled[i].0.clone();
        change(&mut device);
        let all = enrolled.iter().enumerate();
        self.save(all.map(|(j, (d, _))| if i == j { &device } else { d }))
            .await?;
        enrolled[i].0 = device;
        self.changed.send_replace(());
        Ok(())
    }

    /// Gives `name` its notification token and takes that token from any other device, so a
    /// phone paired again under another name is notified once.
    pub(crate) async fn register(&self, name: &str, push: Push) -> Result<(), Error> {
        let mut enrolled = self.enrolled.write().await;
        if !enrolled.iter().any(|(d, _)| d.name == name) {
            return Ok(());
        }
        let devices: Vec<Device> = (enrolled.iter().map(|(d, _)| d.clone()))
            .map(|mut d| {
                match d.name == name {
                    true => d.push = Some(push.clone()),
                    false => drop(d.push.take_if(|p| p.token == push.token)),
                }
                d
            })
            .collect();
        self.save(devices.iter()).await?;
        for ((old, _), new) in enrolled.iter_mut().zip(devices) {
            *old = new;
        }
        self.changed.send_replace(());
        Ok(())
    }

    /// Each device's notification token and what its notifications bring, by its name.
    pub(crate) async fn pushes(&self) -> Vec<(String, Push, Notifications)> {
        let enrolled = self.enrolled.read().await;
        let pushes = enrolled.iter().filter_map(|(d, _)| {
            let notifications = d.notifications.clone().unwrap_or_default();
            Some((d.name.clone(), d.push.clone()?, notifications))
        });
        pushes.collect()
    }

    /// Marks `session`'s stream open on `device` until the answer is dropped.
    pub(crate) fn stream(&self, device: Option<String>, session: &SessionId) -> Arc<()> {
        let held = Arc::new(());
        let mut streams = self.streams.lock().unwrap_or_else(|e| e.into_inner());
        streams.retain(|(.., w)| w.strong_count() > 0);
        streams.push((device, session.clone(), Arc::downgrade(&held)));
        held
    }

    /// The enrolled devices with `session`'s stream open.
    pub(crate) fn watching(&self, session: &SessionId) -> Vec<String> {
        let streams = self.streams.lock().unwrap_or_else(|e| e.into_inner());
        let here = streams
            .iter()
            .filter(|(_, s, w)| s == session && w.strong_count() > 0);
        here.filter_map(|(d, ..)| d.clone()).collect()
    }

    /// A notification after device settings are durably changed.
    pub(crate) fn changes(&self) -> watch::Receiver<()> {
        self.changed.subscribe()
    }

    /// Every device's followed sessions, by its name.
    pub(crate) async fn follows(&self) -> Vec<(String, Follow)> {
        let enrolled = self.enrolled.read().await;
        let follows = enrolled
            .iter()
            .flat_map(|(d, _)| d.follows.iter().map(|f| (d.name.clone(), f.clone())));
        follows.collect()
    }

    /// Removes all uses of a registration its provider calls invalid.
    pub(crate) async fn forget(&self, name: &str, push: &Push) -> Result<(), Error> {
        self.change(name, |d| {
            d.push.take_if(|p| p == push);
            d.follows.retain(|f| f.push != *push);
        })
        .await
    }

    /// Notification preferences for this enrolled device, or the defaults after revocation.
    pub(crate) async fn notifications(&self, name: &str) -> Notifications {
        let enrolled = self.enrolled.read().await;
        enrolled
            .iter()
            .find(|(d, _)| d.name == name)
            .and_then(|(d, _)| d.notifications.clone())
            .unwrap_or_default()
    }

    /// The enrolled devices without private keys or notification tokens.
    pub(crate) async fn list(&self) -> Vec<Device> {
        let enrolled = self.enrolled.read().await;
        enrolled.iter().map(|(d, _)| public(d)).collect()
    }

    /// The caller's enrollment without private fields.
    pub(crate) async fn identity(&self, name: &str) -> Option<Device> {
        self.enrolled
            .read()
            .await
            .iter()
            .find(|(d, _)| d.name == name)
            .map(|(d, _)| public(d))
    }

    /// The authenticated device and its revocation receiver; None for an unknown key.
    pub(crate) async fn authorize(&self, given: &[u8]) -> Option<(Gone, Option<String>)> {
        let mut found =
            same(given, self.local.as_bytes()).then(|| (self.forever.subscribe(), None));
        for (device, gone) in self.enrolled.read().await.iter() {
            if same(given, device.key.as_deref().unwrap_or_default().as_bytes()) {
                found = Some((gone.subscribe(), Some(device.name.clone())));
            }
        }
        found
    }

    /// Resolves an authenticated network key to the same identity and revocation as HTTP keys.
    pub(crate) async fn network(&self, key: iroh::PublicKey) -> Option<(Gone, Option<String>)> {
        self.enrolled
            .read()
            .await
            .iter()
            .find(|(d, _)| d.keys.contains(&key))
            .map(|(d, gone)| (gone.subscribe(), Some(d.name.clone())))
    }

    /// API connections need an enrolled key, and incoming ones a free place; incoming pairing
    /// needs an outstanding code. Each accepted connection, in either direction, is
    /// registered under the enrollment lock, so a removal saved meanwhile still closes it.
    async fn admit(&self, connection: &Connection) -> Result<(), u32> {
        let (key, alpn) = (connection.remote_id(), connection.alpn());
        let (incoming, pairing) = (
            connection.side() == Side::Server,
            alpn == razor_network::PAIRING,
        );
        // Asked before the enrollment lock, which a pairing being saved holds after this one.
        if incoming && pairing && !self.pairing.lock().await.as_ref().is_some_and(|o| o.live()) {
            return Err(1);
        }
        let enrolled = self.enrolled.read().await;
        let known = enrolled.iter().any(|(d, _)| d.keys.contains(&key));
        if !pairing && (alpn != razor_network::API || !known) {
            return Err(if alpn == razor_network::API {
                razor_network::ADMISSION_REFUSED
            } else {
                1
            });
        }
        let mut open = 0;
        let mut connections = self.connections.lock().unwrap_or_else(|e| e.into_inner());
        connections.retain(|weak| {
            weak.upgrade().is_some_and(|c| {
                let live = c.close_reason().is_none();
                let alike = c.alpn() == alpn && (pairing || c.remote_id() == key);
                open += usize::from(live && alike && c.side() == Side::Server);
                live
            })
        });
        let most = if pairing {
            pairing::CONNECTIONS
        } else {
            CONNECTIONS
        };
        if incoming && open >= most {
            return Err(razor_network::ADMISSION_BUSY);
        }
        connections.push(connection.weak_handle());
        Ok(())
    }

    /// Closes the API connections of `keys`, both ways.
    fn close(&self, keys: &[iroh::PublicKey], reason: &[u8]) {
        let mut connections = self.connections.lock().unwrap_or_else(|e| e.into_inner());
        connections.retain(|weak| {
            weak.upgrade().is_some_and(|c| {
                if c.alpn() == razor_network::API && keys.contains(&c.remote_id()) {
                    c.close(1u32.into(), reason);
                }
                c.close_reason().is_none()
            })
        });
    }

    /// Validates and durably enrolls one program by its bearer key, which works on the local
    /// socket alone; publication follows the successful rename.
    pub(crate) async fn enroll(&self, name: String, key: Option<String>) -> Result<Device, Error> {
        let refuse = |status, sentence| Err(Error::Enrollment(status, sentence));
        if !valid_name(&name) {
            let sentence =
                format!("A device name is 1 to 32 lowercase letters, digits and -, not {name:?}.");
            return refuse(StatusCode::BAD_REQUEST, sentence);
        }
        if key.as_ref().is_some_and(|k| !valid_key(k)) {
            let sentence = "A pairing key is the 64 hex digits the other server printed.".into();
            return refuse(StatusCode::BAD_REQUEST, sentence);
        }
        let mut enrolled = self.enrolled.write().await;
        if enrolled.iter().any(|(d, _)| d.name == name) {
            let sentence = format!("A device named {name} is enrolled already; remove it first.");
            return refuse(StatusCode::CONFLICT, sentence);
        }
        if enrolled.len() >= MOST {
            let sentence = format!("{MOST} devices are enrolled already; remove one first.");
            return refuse(StatusCode::CONFLICT, sentence);
        }
        let key = match key {
            Some(key) => key,
            None => fresh(32)?,
        };
        let (added, key) = (now(), Some(key));
        let device = Device {
            name,
            added,
            key,
            ..Device::default()
        };
        self.save(enrolled.iter().map(|(d, _)| d).chain([&device]))
            .await?;
        enrolled.push((device.clone(), watch::channel(()).0));
        Ok(device)
    }

    /// Persists removal before dropping the sender that ends this device's streams.
    pub(crate) async fn revoke(&self, name: &str) -> Result<(), Error> {
        let mut enrolled = self.enrolled.write().await;
        let Some(i) = enrolled.iter().position(|(d, _)| d.name == name) else {
            let sentence = format!("No device named {name} is enrolled.");
            return Err(Error::Enrollment(StatusCode::NOT_FOUND, sentence));
        };
        let rest = enrolled.iter().enumerate().filter(|(j, _)| *j != i);
        self.save(rest.map(|(_, (d, _))| d)).await?;
        // Close with the reason before dropping the sender, which can end an HTTP body cleanly.
        self.close(&enrolled[i].0.keys, b"Device removed");
        enrolled.remove(i);
        Ok(())
    }

    /// Rewrite `devices.json` (mode 0600) by write-and-rename, so a crash leaves the old list or
    /// the new one.
    async fn save(&self, devices: impl Iterator<Item = &Device>) -> Result<(), Error> {
        let devices: Vec<&Device> = devices.collect();
        let new = self.path.with_extension("json.new");
        let write = async {
            let mut file = OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .mode(0o600)
                .open(&new)
                .await?;
            file.write_all(&serde_json::to_vec(&devices)?).await?;
            file.sync_all().await?;
            tokio::fs::rename(&new, &self.path).await
        };
        write.await.map_err(other("Writing devices.json"))
    }
}

/// The hex in the file at `path`, made at the first start from `bytes` random bytes (mode 0600).
pub(crate) async fn kept(
    path: &FilePath,
    bytes: usize,
    context: &'static str,
) -> Result<String, Error> {
    let made = match tokio::fs::read_to_string(path).await {
        Ok(text) => return Ok(text.trim().to_string()),
        Err(e) if e.kind() == ErrorKind::NotFound => fresh(bytes)?,
        Err(e) => return Err(other(context)(e)),
    };
    let mut options = OpenOptions::new();
    let options = options.create_new(true).write(true).mode(0o600);
    let mut file = options.open(path).await.map_err(other(context))?;
    file.write_all(made.as_bytes())
        .await
        .map_err(other(context))?;
    Ok(made)
}

/// `n` random bytes in hex.
pub(crate) fn fresh(n: usize) -> Result<String, Error> {
    let mut bytes = vec![0u8; n];
    aws_lc_rs::rand::fill(&mut bytes).map_err(other("Making random bytes"))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

/// Every byte is compared, so the time taken says nothing about the key. No key is never one.
fn same(given: &[u8], key: &[u8]) -> bool {
    !given.is_empty()
        && given.len() == key.len()
        && given.iter().zip(key).fold(0, |d, (a, b)| d | (a ^ b)) == 0
}
/// A refused operation with its HTTP status, or a failure carrying its source and context.
#[derive(Debug)]
pub enum Error {
    Refused(String),
    Enrollment(StatusCode, String),
    Other {
        source: Box<dyn std::error::Error + Send + Sync>,
        context: &'static str,
    },
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Refused(sentence) | Self::Enrollment(_, sentence) => f.write_str(sentence),
            Self::Other { source, context } => write!(f, "{context} failed: {source}."),
        }
    }
}
impl std::error::Error for Error {}
pub(crate) fn other<E: std::error::Error + Send + Sync + 'static>(
    context: &'static str,
) -> impl FnOnce(E) -> Error {
    move |source| Error::Other {
        source: Box::new(source),
        context,
    }
}
fn refuse<T>(sentence: String) -> Result<T, Error> {
    Err(Error::Refused(sentence))
}

/// Shared device and discovered-server name rule.
pub(crate) fn valid_name(name: &str) -> bool {
    (1..=32).contains(&name.len())
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// Pairing keys are 32 bytes encoded as lowercase hex.
pub(crate) fn valid_key(key: &str) -> bool {
    key.len() == 64
        && key
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
