//! The API server (docs/design/api.md): HTTP and NDJSON streams under `/v1`, on the local socket
//! and an optional tailnet address, every request carrying a device key. Also its client half (how
//! a command reaches it, `--print`, `razor login`, `razor devices`) and `razor doctor`.

mod call;
mod client;
mod devices;
mod emoji;
mod files;
mod load;
mod marks;
pub mod mcp;
mod meetings;
mod pair;
mod peers;
mod project;
mod push;
mod ready;
mod recap;
mod repos;
mod residents;
mod sessions;
mod speech;
mod status;
mod views;
mod voice;

use crate::controller::{self, Controller};
pub use crate::devices::Error;
use crate::devices::{Devices, Gone, other};
use crate::protocol::{BODY_BYTES, Delivery, EntryId, Live, Login, Mail, Server, SessionId};
use crate::residents::{Deliveries, Error as ResidentError, Residents};
use crate::responses::Responses;
use crate::{config, profiles};
use axum::Router;
use axum::body::Body;
use axum::extract::{DefaultBodyLimit, Extension, Json, Path, Query, RawQuery, Request, State};
use axum::http::{Method, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post, put};
use axum::serve::ListenerExt;
pub use client::{
    Connection, DEVICES_USE, connect, connection, devices, enroll, find, login, newest, revoke,
    turn, voice,
};
use futures_util::{FutureExt, Stream, StreamExt, stream};
pub(crate) use ready::credentials;
pub use ready::doctor;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use status::{identity, label};
use std::collections::{BTreeMap, VecDeque};
use std::convert::Infallible;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::net::{TcpListener, UnixListener};
use tokio::signal::unix::{SignalKind, signal};
use tokio::sync::{Mutex, mpsc, oneshot, watch};

/// A stream's heartbeat period when its reader asks for one: the status stream's `server` line,
/// a session stream's `heartbeat` after that long without a line.
const BEAT: Duration = Duration::from_secs(15);
/// The local socket and the local device's key (mode 0600), in the state directory.
pub(crate) const SOCKET: &str = "server.sock";
pub(crate) use crate::devices::KEY;
/// This home's id as a phone knows it (mode 0600), in the state directory.
const ID: &str = "server.id";
/// The base URL of a client bound to `SOCKET`; the host is not used.
pub(crate) const LOCAL: &str = "http://localhost";
/// Maximum time to finish live calls after shutdown starts.
pub(crate) const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(20);
/// The largest `POST /v1/mail` body: mail's 64 KiB of text, and 8 KiB for the sender, the
/// project, the ids and the text's JSON escapes.
const MAIL_BYTES: usize = (64 << 10) + (8 << 10);

#[derive(Clone)]
struct App {
    home: Arc<PathBuf>,
    peers: Arc<peers::Peers>,
    controller: Controller,
    residents: Residents,
    deliveries: Deliveries,
    responses: Arc<Responses>,
    devices: Arc<Devices>,
    emoji: Arc<emoji::History>,
    views: Arc<views::Kept>,
    meetings: Arc<meetings::Kept>,
    /// This home's id.
    id: Arc<str>,
    /// Public identity fixed at startup; the listener's address once bound.
    name: Arc<str>,
    address: Arc<std::sync::OnceLock<std::net::SocketAddr>>,
    /// Serializes settings and the daily count on their owner through durable replacement.
    voice: Arc<Mutex<()>>,
    /// On the voice owner, the latest input of at most 64 terminals on any server naming it,
    /// expired after a minute on the next sample.
    activity: Arc<Mutex<BTreeMap<String, std::time::Instant>>>,
    /// The newest 64 creations that carried a request id, since the server started.
    creations: Arc<Mutex<VecDeque<(String, SessionId)>>>,
    /// True once the server stops; each live call holds a receiver until it has ended.
    calls: Arc<watch::Sender<bool>>,
}

/// `razor serve`: this home's one server, until SIGTERM.
/// - Raises the soft limit on open files to the hard one, at most 65,536, and holds `server.lock`
///   until the process exits.
/// - Listens on `home/server.sock`, replacing a stale file, and on `server.address`, the tailnet
///   address, when one is set.
/// - Makes `server.id` at the first start and writes `server.json`: `{pid, address}`. The
///   controller has already opened device enrollment and keys through `crate::devices`.
/// - Every request must carry `Authorization: Bearer <key>`: the local key or an enrolled
///   device's. A body is at most 32 MiB.
/// - ChatGPT sign-in shares the controller's Responses client and authentication mutex.
/// - Sends to Apple's push service beside serving.
/// - Returns once SIGTERM comes and the live calls have ended, recorded and charged, or 20
///   seconds have passed.
pub async fn serve(home: PathBuf) -> Result<(), Error> {
    raise_open_files();
    config::make_home(&home).map_err(refused)?;
    // The kernel releases it at process exit, after the runtime and any manager-visible work.
    std::mem::forget(crate::residents::server_lock(&home).map_err(refused)?);
    let mut terminate = signal(SignalKind::terminate()).map_err(refused)?;
    crate::record::backfill(&home).await;
    let responses = Arc::new(Responses::new(home.clone()).map_err(refused)?);
    let controller = Controller::start(home.clone(), responses.clone());
    let controller = controller.await.map_err(refused)?;
    let server = controller.config().await.server;
    let residents = Residents::open(controller.clone());
    tokio::spawn(crate::residents::desktop(home.clone()));
    let shutdown = async move {
        terminate.recv().await;
    };
    let served = listen(home, server, controller, residents, responses, shutdown).await;
    served.map_err(|e| Error::Refused(format!("The API server stopped: {e}")))
}

/// A failure before serving, in its own sentence.
fn refused(e: impl std::fmt::Display) -> Error {
    Error::Refused(e.to_string())
}

/// Every agent holds pipes to its jobs, its MCP servers and its Post watch, and a shell's default
/// (256 on macOS, 1,024 under systemd) runs out near twenty agents, after which records stop
/// opening. Best effort; the limit it ends with goes to server.log, so a server started from any
/// shell can be checked.
fn raise_open_files() {
    let mut limit = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    // SAFETY: getrlimit and setrlimit read and write a struct this function owns.
    if unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut limit) } == 0 {
        limit.rlim_cur = limit.rlim_cur.max(limit.rlim_max.min(65_536));
        // SAFETY: as above; a refusal leaves the limit as it was.
        unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &limit) };
        // SAFETY: as above.
        unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut limit) };
        eprintln!("razor: open files allowed: {}", limit.rlim_cur);
    }
}

/// Serves until `shutdown` resolves and the live calls have ended.
async fn listen(
    home: PathBuf,
    server: config::Server,
    controller: Controller,
    residents: Residents,
    responses: Arc<Responses>,
    shutdown: impl Future<Output = ()>,
) -> Result<(), Error> {
    let (address, deliveries) = (server.address, controller.deliveries());
    if let Some(address) = &address {
        let port = address
            .rsplit_once(':')
            .is_some_and(|(host, port)| !host.is_empty() && port.parse::<u16>().is_ok());
        if !port {
            return Err(Error::Refused(
                "server.address must be a host:port, for example desk:7433.".into(),
            ));
        }
    }
    let name = server.name.into();
    let devices = controller.devices().clone();
    let id = crate::devices::kept(&home.join(ID), 16, "Reading or making server.id").await?;
    let socket = home.join(SOCKET);
    // A stale socket file: the caller holds `server.lock`, so no server answers on it.
    let _ = tokio::fs::remove_file(&socket).await;
    let local = UnixListener::bind(&socket).map_err(|e| {
        Error::Refused(format!("Razor cannot listen on {}: {e}.", socket.display()))
    })?;
    let found = json!({ "pid": std::process::id(), "address": null });
    tokio::fs::write(home.join("server.json"), found.to_string())
        .await
        .map_err(other("Writing server.json"))?;
    let app = App {
        home: Arc::new(home.clone()),
        peers: Arc::default(),
        controller,
        residents,
        deliveries,
        responses,
        devices,
        emoji: Arc::new(emoji::History::open(&home).await),
        views: Arc::new(views::Kept::open(&home).await),
        meetings: Arc::new(meetings::Kept::open().await),
        id: id.into(),
        name,
        address: Arc::default(),
        creations: Arc::default(),
        voice: Arc::default(),
        activity: Arc::default(),
        calls: Arc::new(watch::channel(false).0),
    };
    let speaking = app.clone();
    app.controller
        .children()
        .speak(Box::new(move |id, entry, text, requested| {
            let spoken = speech::spoken(speaking.clone(), id, entry, text, requested);
            Box::pin(spoken.map(|spoken| spoken.map_err(|r| crate::children::Error::Refused(r.1))))
        }));
    let router = routes(&app);
    let local = axum::serve(local, router.clone()).into_future();
    let served = async { tokio::try_join!(local, network(&app, address, router)).map(drop) };
    tokio::select! {
        served = served => served.map_err(other("Serving the API")),
        () = push::send(app.clone()) => Ok(()),
        () = peers::run(app.clone()) => Ok(()),
        () = views::run(app.clone()) => Ok(()),
        () = meetings::run(app.clone()) => Ok(()),
        () = shutdown => {
            app.calls.send_replace(true);
            let _ = tokio::time::timeout(SHUTDOWN_TIMEOUT, app.calls.closed()).await;
            Ok(())
        }
    }
}

/// Every route under `/v1`, behind the device check; a paired session's go on to its owner.
fn routes(app: &App) -> Router {
    Router::new()
        .merge(sessions::routes())
        .route("/v1/repos", get(repos::repos))
        .route("/v1/sessions/{id}/recap", post(recap::recap))
        .route("/v1/sessions/{id}/project", get(project::project))
        .route("/v1/sessions/{id}/marks", get(marks::marks))
        .route("/v1/sessions/{id}/files/{*name}", get(files::file))
        .route("/v1/sessions/{id}/files/{*name}", post(files::upload))
        .route("/v1/sessions/{id}/speech/{entry}", get(speech::speech))
        .route("/v1/voice/activity", put(voice::activity))
        .route(
            "/v1/sessions/{id}/deliveries/{entry}/read",
            put(delivery_read).delete(delivery_read),
        )
        .route(
            "/v1/sessions/{id}/deliveries/{entry}/archive",
            put(delivery_archive).delete(delivery_archive),
        )
        .route_layer(middleware::from_fn_with_state(app.clone(), peers::forward))
        .route("/v1/sessions/{id}/call", get(call::upgrade))
        .merge(meetings::routes())
        .route("/v1/sessions/{id}/voice/{entry}", post(speech::note))
        .route("/v1/views", get(views::route).put(views::route))
        .route("/v1/status", get(status::status))
        .merge(residents::routes())
        .route(
            "/v1/children",
            post(peers::child).layer(DefaultBodyLimit::max(512 << 10)),
        )
        .route(
            "/v1/permissions",
            get(async || Json(crate::residents::permissions(None).await)).post(
                async |State(app): State<App>| {
                    Json(crate::residents::permissions(Some(&app.home)).await)
                },
            ),
        )
        .route("/v1/emoji", get(emoji::history).post(emoji::used))
        .route("/v1/deliveries", get(delivery_list))
        .route("/v1/voice", get(voice::get).put(voice::put))
        .route("/v1/voice/voices", get(voice::voices))
        .route("/v1/voice/reserve", post(voice::reserve))
        .route("/v1/devices", get(devices::list).post(devices::enroll))
        .route("/v1/discover", get(pair::discover))
        .route("/v1/device", get(devices::identity))
        .route(
            "/v1/pair",
            get(pair::identity)
                .post(pair::hold)
                .layer(DefaultBodyLimit::max(1024)),
        )
        .route("/v1/devices/{name}", delete(devices::revoke))
        .route(
            "/v1/devices/self/push",
            put(devices::register).delete(devices::unregister),
        )
        .route(
            "/v1/devices/self/notifications",
            get(devices::notifications).put(devices::set_notifications),
        )
        .route(
            "/v1/sessions/{id}/follow",
            put(devices::follow).delete(devices::unfollow),
        )
        .route(
            "/v1/mail",
            post(mail).layer(DefaultBodyLimit::max(MAIL_BYTES)),
        )
        .route("/v1/login", post(sign_in))
        .route("/v1/mcp", post(mcp))
        .route("/v1/mcp/callback", get(mcp_callback))
        .fallback(|| async { Refusal(StatusCode::NOT_FOUND, "There is no such route.".into()) })
        .layer(DefaultBodyLimit::max(BODY_BYTES))
        .layer(middleware::from_fn(refusals))
        .layer(middleware::from_fn_with_state(
            app.devices.clone(),
            devices::authorize,
        ))
        .with_state(app.clone())
}

/// Serves `router` on `server.address`, trying again every 5 seconds until it can listen there,
/// then publishes the bound address in `server.json`; with no address, never resolves.
async fn network(app: &App, address: Option<String>, router: Router) -> io::Result<()> {
    let Some(address) = address else {
        return std::future::pending().await;
    };
    let mut first = true;
    let remote = loop {
        match TcpListener::bind(&address).await {
            Ok(remote) => break remote,
            Err(e) => {
                if first {
                    eprintln!(
                        "Razor is not listening on server.address {address}: {e}; retrying every 5 seconds."
                    );
                    first = false;
                }
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    };
    let bound = remote.local_addr()?;
    let _ = app.address.set(bound);
    if !first {
        eprintln!("Razor is now listening on server.address {address} ({bound}).");
    }
    let found = json!({"pid": std::process::id(), "address": bound.to_string()});
    tokio::fs::write(app.home.join("server.json"), found.to_string()).await?;
    // Streaming small writes must not wait for a peer's delayed acknowledgment, and a
    // peer that vanished without closing (a phone that lost its network) is let go about
    // two minutes after it last answered: keepalive probes an idle connection after 60
    // seconds, every 15, four times; on Linux, writes unacknowledged for 120 seconds end
    // one that is never idle, as a stream with heartbeats is, which keepalive never probes.
    let remote = remote.tap_io(|tcp| {
        let (tcp, secs) = (socket2::SockRef::from(&*tcp), Duration::from_secs);
        let probes = socket2::TcpKeepalive::new().with_time(secs(60));
        let probes = probes.with_interval(secs(15)).with_retries(4);
        drop(tcp.set_tcp_nodelay(true));
        drop(tcp.set_tcp_keepalive(&probes));
        #[cfg(target_os = "linux")]
        drop(tcp.set_tcp_user_timeout(Some(secs(120))));
    });
    axum::serve(
        remote,
        router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .into_future()
    .await
}

/// A client's own id for an input or a creation: 1 to 64 ASCII letters, digits and `-`.
fn request(request: Option<&str>) -> Result<(), Refusal> {
    match request.filter(|r| {
        !(1..=64).contains(&r.len()) || r.bytes().any(|b| !b.is_ascii_alphanumeric() && b != b'-')
    }) {
        Some(r) => Err(bad(format!(
            "A request id is 1 to 64 ASCII letters, digits and -, not {r:?}."
        ))),
        _ => Ok(()),
    }
}

/// A refusal: a status and one sentence, sent as `{"error": "…"}`.
struct Refusal(StatusCode, String);
type Reply<T = Value> = Result<Json<T>, Refusal>;

impl IntoResponse for Refusal {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "error": self.1 }))).into_response()
    }
}

impl From<controller::Error> for Refusal {
    fn from(e: controller::Error) -> Refusal {
        let status = match &e {
            controller::Error::NoSession(_) => StatusCode::NOT_FOUND,
            // A file the client named that is not there.
            controller::Error::Other { source, .. }
                if (source.downcast_ref::<io::Error>())
                    .is_some_and(|e| e.kind() == io::ErrorKind::NotFound) =>
            {
                StatusCode::NOT_FOUND
            }
            controller::Error::Full(_) | controller::Error::Busy(_) => StatusCode::CONFLICT,
            controller::Error::Invalid(_) => StatusCode::BAD_REQUEST,
            controller::Error::Other { .. } => StatusCode::INTERNAL_SERVER_ERROR,
        };
        Refusal(status, e.to_string())
    }
}

impl From<ResidentError> for Refusal {
    fn from(e: ResidentError) -> Refusal {
        let status = match &e {
            ResidentError::NoResident(_) | ResidentError::NoProject(_) => StatusCode::NOT_FOUND,
            ResidentError::Refused(_) => StatusCode::BAD_REQUEST,
            ResidentError::Other { .. } => StatusCode::INTERNAL_SERVER_ERROR,
        };
        Refusal(status, e.to_string())
    }
}

impl From<crate::children::Error> for Refusal {
    fn from(e: crate::children::Error) -> Refusal {
        let status = match &e {
            crate::children::Error::Refused(_) => StatusCode::BAD_REQUEST,
            crate::children::Error::Other { .. } => StatusCode::INTERNAL_SERVER_ERROR,
        };
        Refusal(status, e.to_string())
    }
}

impl From<crate::devices::Error> for Refusal {
    fn from(e: crate::devices::Error) -> Self {
        let status = match &e {
            crate::devices::Error::Enrollment(status, _) => *status,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        Refusal(status, e.to_string())
    }
}

fn bad(sentence: impl Into<String>) -> Refusal {
    Refusal(StatusCode::BAD_REQUEST, sentence.into())
}

/// The sign-in runs on its own task; leaving or removing its device cancels it.
async fn sign_in(State(app): State<App>, Extension(gone): Extension<Gone>) -> Response {
    let (show, shown) = mpsc::channel(8);
    let (stop, stopped) = watch::channel(false);
    let (mut end, ended) = oneshot::channel();
    tokio::spawn(async move {
        let login = app.responses.login(show, stopped);
        tokio::pin!(login);
        let result = tokio::select! {
            result = &mut login => result,
            () = end.closed() => {
                let _ = stop.send(true);
                login.await
            }
        };
        let _ = end.send(result);
    });
    let last = ended.map(|result| match result {
        Ok(Ok(())) => Login::Done,
        Ok(Err(e)) => Login::Failed {
            error: e.to_string(),
        },
        Err(_) => Login::Failed {
            error: "The sign-in stopped before it ended.".into(),
        },
    });
    let lines = received(shown)
        .map(|text| Login::Show { text })
        .chain(stream::once(last))
        .filter_map(|line| async move { line_of(&line) });
    ndjson(lines, gone)
}

#[derive(Deserialize)]
struct McpRequest {
    cwd: PathBuf,
    session: Option<SessionId>,
    #[serde(flatten)]
    operation: crate::protocol::McpOperation,
}

async fn mcp(State(app): State<App>, Json(request): Json<McpRequest>) -> Reply {
    let answer = app
        .controller
        .mcp(&request.cwd, request.session.as_ref(), request.operation)
        .await?;
    Ok(Json(answer))
}

async fn mcp_callback(State(app): State<App>, RawQuery(query): RawQuery) -> Reply {
    let query = query.unwrap_or_default();
    let parsed = reqwest::Url::parse(&format!("http://localhost/?{query}")).map_err(|_| {
        bad("The MCP callback query is invalid; paste the complete redirect address.")
    })?;
    let redirect = parsed
        .query_pairs()
        .find(|(key, _)| key == "redirect")
        .map(|(_, value)| value.into_owned());
    let redirect = match redirect {
        Some(redirect) => redirect,
        None => {
            let mut url = callback(&app).await.map_err(|e| bad(e.to_string()))?;
            url.set_query(Some(&query));
            url.into()
        }
    };
    app.controller.mcp_complete(&redirect).await?;
    Ok(Json(json!({"signed_in": true})))
}

async fn callback(app: &App) -> Result<reqwest::Url, crate::tools::Error> {
    crate::tools::callback(app.controller.config().await.server.https_url.as_deref())
}

/// A person's input that is one landed sign-in address completes its sign-in as the callback
/// does, answering the ask of whichever session began it; its code reaches neither the record
/// nor the model.
async fn landed(app: &App, id: &SessionId, text: &str) -> Result<bool, Refusal> {
    let text = text.trim();
    let Ok(mut url) = reqwest::Url::parse(text) else {
        return Ok(false);
    };
    url.set_query(None);
    if text.contains(char::is_whitespace) || callback(app).await.ok() != Some(url) {
        return Ok(false);
    }
    app.controller.mcp_complete(text).await?;
    // The terminal saved the address, code and all, as the draft before sending it.
    app.controller.set_draft(id, String::new()).await?;
    Ok(true)
}

/// axum's own refusals (a malformed body, path or query; a body over its limit) are plain text:
/// send them as every other refusal is sent.
async fn refusals(request: Request, next: Next) -> Response {
    let response = next.run(request).await;
    let plain = (response.headers().get(header::CONTENT_TYPE))
        .is_some_and(|t| t.as_bytes().starts_with(b"text/plain"));
    if response.status().is_success() || !plain {
        return response;
    }
    let status = response.status();
    let text = axum::body::to_bytes(response.into_body(), 4096).await;
    let text = String::from_utf8_lossy(&text.unwrap_or_default()).into_owned();
    let text = text.trim_end_matches('.');
    Refusal(status, format!("The request was refused: {text}.")).into_response()
}

/// The query of every listing and stream; each route reads its own fields.
#[derive(Deserialize)]
struct Page {
    cwd: Option<PathBuf>,
    /// The sessions list: only this resident's threads.
    resident: Option<SessionId>,
    /// The sessions list: fresh runs too; by default only with `resident`.
    jobs: Option<bool>,
    session: Option<SessionId>,
    before: Option<u64>,
    after: Option<u64>,
    /// The last entry a reader saw.
    entry: Option<u64>,
    limit: Option<usize>,
    #[serde(default)]
    local: bool,
    #[serde(default)]
    heartbeat: bool,
    #[serde(default)]
    views: bool,
    #[serde(default)]
    terminal: bool,
    #[serde(default)]
    follow: bool,
    #[serde(default)]
    ready: bool,
    /// A session's stream and pages: entries keep their `native`.
    #[serde(default)]
    native: bool,
    filter: Option<String>,
    q: Option<String>,
}

/// Mail from a paired server, or a local program, to the one agent here its `to` names.
async fn mail(
    State(app): State<App>,
    Extension(devices::Enrolled(server)): Extension<devices::Enrolled>,
    Json(mail): Json<Mail>,
) -> Reply {
    let (session, entry) =
        crate::children::receive(&app.controller, mail, server.as_deref()).await?;
    Ok(Json(json!({ "session": session, "entry": entry })))
}

/// Newest first; cursors retain each delivery's original position when read.
async fn delivery_list(State(app): State<App>, Query(q): Query<Page>) -> Reply<Vec<Delivery>> {
    let limit = q.limit.unwrap_or(50).clamp(1, 256);
    let words: Vec<_> =
        q.q.unwrap_or_default()
            .split_whitespace()
            .map(str::to_lowercase)
            .collect();
    let rows = app
        .deliveries
        .list(q.session.zip(q.before.map(EntryId)), limit, &words)
        .await?;
    Ok(Json(if q.local || q.before.is_some() {
        rows
    } else {
        app.peers.deliveries(rows, limit, &words).await
    }))
}

async fn delivery_read(
    State(app): State<App>,
    Path((id, entry)): Path<(SessionId, EntryId)>,
    method: Method,
) -> Reply {
    app.deliveries
        .read(&id, entry, method == Method::PUT)
        .await?;
    Ok(Json(json!({})))
}

async fn delivery_archive(
    State(app): State<App>,
    Path((id, entry)): Path<(SessionId, EntryId)>,
    method: Method,
) -> Reply {
    app.deliveries
        .archive(&id, entry, method == Method::PUT)
        .await?;
    Ok(Json(json!({})))
}

const NDJSON: &str = "application/x-ndjson";

/// A channel's lines as a stream.
fn received<T: Send + 'static>(mut lines: mpsc::Receiver<T>) -> impl Stream<Item = T> + Send {
    stream::poll_fn(move |cx| lines.poll_recv(cx))
}

/// Replaces one home file only after its contents and the containing directory are synced.
async fn save(home: &std::path::Path, name: &str, bytes: &[u8]) -> std::io::Result<()> {
    let path = home.join(name);
    let new = path.with_extension("json.new");
    tokio::fs::write(&new, bytes).await?;
    tokio::fs::File::open(&new).await?.sync_all().await?;
    tokio::fs::rename(&new, path).await?;
    tokio::fs::File::open(home).await?.sync_all().await
}

/// What `save` keeps in the home file `name`, read up to `cap` bytes; the default when the file
/// is missing, unreadable or longer.
async fn load<T: DeserializeOwned + Default>(home: &std::path::Path, name: &str, cap: u64) -> T {
    let mut bytes = Vec::new();
    if let Ok(file) = tokio::fs::File::open(home.join(name)).await {
        let _ = file.take(cap).read_to_end(&mut bytes).await;
    }
    serde_json::from_slice(&bytes).unwrap_or_default()
}

/// The longest start of `text` within `max` bytes that ends at a character's boundary.
fn cut(text: &str, max: usize) -> &str {
    &text[..text.floor_char_boundary(max)]
}

/// One JSON line; `None` only for a value JSON cannot hold (a path that is not UTF-8).
fn line_of(value: &impl Serialize) -> Option<Arc<str>> {
    serde_json::to_string(value).ok().map(Arc::from)
}

/// An NDJSON response of `lines`, which ends when the device that asked is removed.
fn ndjson(lines: impl Stream<Item = Arc<str>> + Send + 'static, mut gone: Gone) -> Response {
    let gone = async move { gone.changed().await };
    let body = (lines.take_until(gone)).map(|line| Ok::<_, Infallible>(format!("{line}\n")));
    ([(header::CONTENT_TYPE, NDJSON)], Body::from_stream(body)).into_response()
}
