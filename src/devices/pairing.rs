//! Pairing by code: one outstanding code, spent under one lock with the enrollment it makes.
use super::*;
use aws_lc_rs::constant_time::verify_slices_are_equal;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use iroh::{PublicKey, RelayUrl};
use tokio::time::{Instant, timeout};

/// Ten minutes leaves time to move between devices without a long-lived credential.
const LIFETIME: Duration = Duration::from_secs(600);
/// Ten seconds covers relay setup and one small exchange without retaining idle strangers.
pub(crate) const DEADLINE: Duration = Duration::from_secs(10);
/// Four simultaneous pairers leave room for a retry while bounding unauthenticated work.
pub(super) const CONNECTIONS: usize = 4;
/// Eight relays leave room beyond iroh's default map; each URL is at most 256 bytes.
const RELAYS: usize = 8;
/// Four KiB fits the secret, the device's extension keys and eight relay URLs.
pub(crate) const BYTES: usize = 4096;
/// Version, key, secret and eight 256-byte URLs in unpadded base64url, plus the prefix.
pub(crate) const CODE_BYTES: usize = 2825;

/// The outstanding code. Its `devices add` holds the only receiver of `done`, so the code is
/// withdrawn when that command goes.
pub(super) struct Offer {
    name: String,
    secret: [u8; 16],
    expires: Instant,
    done: watch::Sender<String>,
}

impl Offer {
    /// Unexpired and still waited for.
    pub(super) fn live(&self) -> bool {
        self.expires > Instant::now() && !self.done.is_closed()
    }
}

/// A code as `razor1-` and unpadded base64url of version 1, the key, the secret, then each relay
/// as a two-byte big-endian length and its URL: one line that survives copying and a QR code.
pub(crate) struct Code {
    pub(crate) key: PublicKey,
    pub(crate) relays: Vec<RelayUrl>,
    secret: [u8; 16],
}

impl Code {
    fn text(&self) -> String {
        let mut bytes = [&[1][..], &self.key.as_bytes()[..], &self.secret[..]].concat();
        for relay in &self.relays {
            let relay = relay.as_str().as_bytes();
            bytes.extend((relay.len() as u16).to_be_bytes().iter().chain(relay));
        }
        format!("razor1-{}", URL_SAFE_NO_PAD.encode(bytes))
    }

    pub(crate) fn parse(text: &str) -> Result<Self, Error> {
        let parse = || {
            let text = text
                .strip_prefix("razor1-")
                .filter(|_| text.len() <= CODE_BYTES)?;
            let bytes = URL_SAFE_NO_PAD.decode(text).ok()?;
            let (head, mut rest) = bytes.split_at_checked(49)?;
            let key = PublicKey::from_bytes(head[1..33].try_into().ok()?).ok()?;
            let mut relays = Vec::new();
            while let [high, low, tail @ ..] = rest {
                let n = usize::from(u16::from_be_bytes([*high, *low]));
                let fits = n <= 256 && relays.len() < RELAYS;
                let (relay, tail) = tail.split_at_checked(n).filter(|_| fits)?;
                relays.push(std::str::from_utf8(relay).ok()?.parse().ok()?);
                rest = tail;
            }
            let secret = head[33..].try_into().ok()?;
            (head[0] == 1 && rest.is_empty()).then_some(Self {
                key,
                relays,
                secret,
            })
        };
        let invalid = "The pairing code is invalid; paste a fresh code from razor devices add.";
        parse().ok_or_else(|| Error::Refused(invalid.into()))
    }

    /// What a joining server sends for this code.
    pub(crate) fn exchange(&self, key: PublicKey, relays: Vec<RelayUrl>, id: String) -> Exchange {
        let (secret, keys) = (URL_SAFE_NO_PAD.encode(self.secret), vec![key]);
        Exchange {
            server_id: Some(id),
            secret,
            keys,
            relays,
        }
    }
}

/// A joining device's secret, keys and relays; a server adds its id. The connection proves one
/// of the keys; the others are its extensions', joining the same enrollment.
#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) struct Exchange {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    server_id: Option<String>,
    secret: String,
    keys: Vec<PublicKey>,
    relays: Vec<RelayUrl>,
}

/// At most eight relays of at most 256 bytes, and, with `allowed`, only those: enforced at pairing
/// alone.
pub(crate) fn relays(given: &[RelayUrl], allowed: Option<&[RelayUrl]>) -> Result<(), Error> {
    if given.len() > RELAYS || given.iter().any(|r| r.as_str().len() > 256) {
        return refuse("Pairing accepts at most eight relay URLs of at most 256 bytes each; shorten network.relays.".into());
    }
    if let Some(allowed) = allowed
        && let Some(relay) = given.iter().find(|r| !allowed.contains(r))
    {
        let shown = crate::config::shown(relay);
        // The same relay with another query: a password that differs, which `config get` hides.
        return refuse(
            if allowed.iter().any(|a| crate::config::shown(a) == shown) {
                format!(
                    "Relay {shown} has another password in this device's network.relays than in the other device's; make them match in config.toml on both machines (razor config get does not show passwords), then make a fresh code."
                )
            } else {
                format!(
                    "Relay {shown} is outside this device's network.relays; include it there or change the other device's network.relays and make a fresh code."
                )
            },
        );
    }
    Ok(())
}

/// A server id, as `server.id` holds it.
pub(crate) fn server_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit())
}

impl Devices {
    /// A fresh code that replaces the outstanding one, ending its `devices add` with a line
    /// saying so, and the receiver that hears how this one ends.
    pub(crate) async fn offer(
        &self,
        name: String,
        key: PublicKey,
        relays: Vec<RelayUrl>,
    ) -> Result<(String, watch::Receiver<String>), Error> {
        if !valid_name(&name) {
            return refuse("A device name is 1 to 32 lowercase letters, digits and -.".into());
        }
        self::relays(&relays, None)?;
        let mut secret = [0; 16];
        aws_lc_rs::rand::fill(&mut secret).map_err(other("Making a pairing code"))?;
        let (done, receiver) = watch::channel(String::new());
        let expires = Instant::now() + LIFETIME;
        let offer = Offer {
            name,
            secret,
            expires,
            done,
        };
        if let Some(previous) = self.pairing.lock().await.replace(offer) {
            previous.done.send_replace("Pairing code replaced.".into());
        }
        Ok((
            Code {
                key,
                relays,
                secret,
            }
            .text(),
            receiver,
        ))
    }

    /// The line `devices add` ends with: who joined, or that the code was replaced or expired. A
    /// pairing accepted just before the expiry is saved first.
    pub(crate) async fn outcome(&self, mut done: watch::Receiver<String>) -> String {
        if timeout(LIFETIME, done.changed()).await.is_err() {
            let _saved = self.pairing.lock().await;
            if done.borrow().is_empty() {
                return "Pairing code expired.".into();
            }
        }
        done.borrow().clone()
    }

    /// Checks the secret, enrolls the device whose connection proved `key` and spends the code,
    /// under the code's lock and in a task of its own, so a pairer that leaves cannot cut it
    /// short. A refusal leaves the code outstanding.
    pub(crate) async fn accept_pairing(
        self: &Arc<Self>,
        key: PublicKey,
        mut request: Exchange,
        allowed: Option<Vec<RelayUrl>>,
    ) -> Result<(), Error> {
        let devices = self.clone();
        let accepting = async move {
            let mut outstanding = devices.pairing.lock().await;
            let secret = URL_SAFE_NO_PAD.decode(&request.secret).unwrap_or_default();
            let matches =
                |o: &&Offer| o.live() && verify_slices_are_equal(&secret, &o.secret).is_ok();
            let Some(offer) = outstanding.as_ref().filter(matches) else {
                let sentence =
                    "Pairing code refused or expired; make a fresh code on the other device.";
                return refuse(sentence.into());
            };
            relays(&request.relays, allowed.as_deref())?;
            if request.keys.len() > 8 || !request.keys.contains(&key) {
                return refuse(
                    "Pairing needs the authenticated key and at most eight device keys.".into(),
                );
            }
            if !request.server_id.as_deref().is_none_or(server_id) {
                return refuse(
                    "Pairing needs a valid server identity; make a fresh code and try again."
                        .into(),
                );
            }
            request.keys.retain(|k| *k != key);
            request.keys.insert(0, key);
            let name = offer.name.clone();
            let (keys, relays, id) = (request.keys, request.relays, request.server_id);
            devices
                .enroll_network(name.clone(), keys, relays, id)
                .await?;
            if let Some(offer) = outstanding.take() {
                offer.done.send_replace(format!("{name} joined"));
            }
            Ok(())
        };
        tokio::spawn(accepting)
            .await
            .map_err(other("Finishing pairing"))?
    }

    /// Refuses an occupied name before a join can spend the other machine's code.
    pub(crate) async fn check_network_name(&self, name: &str, key: PublicKey) -> Result<(), Error> {
        let enrolled = self.enrolled.read().await;
        check_name(&enrolled, name, Some(&key))
    }

    /// Enrolls `keys` as `name`. The first is the key a connection proved. Pairing again takes
    /// the place of the enrollment holding that key, as pairing does of one under `name` made
    /// before pairing by code, which holds no network key: the new one carries the old one's
    /// settings, all but the bearer key it answered to, and the old one's connections close.
    /// Another device's name or keys are refused.
    pub(crate) async fn enroll_network(
        &self,
        name: String,
        keys: Vec<PublicKey>,
        relays: Vec<RelayUrl>,
        server_id: Option<String>,
    ) -> Result<(), Error> {
        let mut enrolled = self.enrolled.write().await;
        let holds = |d: &Device| keys.first().is_some_and(|k| d.keys.contains(k));
        let previous = (enrolled.iter())
            .position(|(d, _)| d.name == name && d.keys.is_empty())
            .or_else(|| enrolled.iter().position(|(d, _)| holds(d)));
        check_name(&enrolled, &name, keys.first())?;
        if let Some((device, _)) = enrolled.iter().enumerate().find_map(|(i, d)| {
            (Some(i) != previous && d.0.keys.iter().any(|k| keys.contains(k))).then_some(d)
        }) {
            return refuse(format!(
                "A key is enrolled under {}; remove that device first.",
                device.name
            ));
        }
        if previous.is_none() && enrolled.len() == MOST {
            return refuse(format!(
                "{MOST} devices are enrolled already; remove one first."
            ));
        }
        let new = || Device {
            added: now(),
            ..Device::default()
        };
        let device = Device {
            name,
            key: None,
            keys,
            relays,
            server_id,
            ..previous.map_or_else(new, |i| enrolled[i].0.clone())
        };
        let kept = enrolled
            .iter()
            .enumerate()
            .filter(|(i, _)| Some(*i) != previous);
        self.save(kept.map(|(_, (d, _))| d).chain([&device]))
            .await?;
        if let Some(i) = previous {
            self.close(&enrolled[i].0.keys, b"Device paired again");
            enrolled.remove(i);
        }
        enrolled.push((device, watch::channel(()).0));
        self.grown.notify_one();
        Ok(())
    }
}

/// A name is taken by an enrollment holding network keys that are not `key`'s.
fn check_name(
    enrolled: &[(Device, watch::Sender<()>)],
    name: &str,
    key: Option<&PublicKey>,
) -> Result<(), Error> {
    if enrolled.iter().any(|(d, _)| {
        d.name == name && !d.keys.is_empty() && !key.is_some_and(|k| d.keys.contains(k))
    }) {
        return refuse(format!(
            "A device named {name} is enrolled already; remove it or choose another name."
        ));
    }
    Ok(())
}
