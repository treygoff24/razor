//! Push services: an alert to each device with a token whose level and mutes let it
//! through when a question opens, a root notes something to the person or delivers, or a
//! top-level session that is not a resident finishes its turn or gets stuck; and each followed
//! session's Live Activity updates. Events come from the controller's status stream and the
//! deliveries' broadcast. No alert carries an agent's words, and no key, token or signature
//! reaches a record, a log or an error.

use super::{App, Error};
use crate::config::{self, PushKey};
use crate::protocol::{
    AgentView, Body, CloseReason, End, Entry, EntryId, Environment, Follow, Following, Input, Live,
    Millis, NotificationLevel, Push, PushService, SessionId, State, now,
};
use crate::record::Summary;
use aws_lc_rs::rand::SystemRandom;
use aws_lc_rs::signature::{ECDSA_P256_SHA256_FIXED_SIGNING, EcdsaKeyPair};
use axum::http::StatusCode;
use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use futures_util::StreamExt;
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};
use std::time::Duration;
use tokio::sync::{Mutex, broadcast::error::RecvError, mpsc};
use tokio::time::Instant;

/// At most this many sends wait, and this many run at once.
const WAITING: usize = 256;
const AT_ONCE: usize = 32;
/// A provider token is made again this long after it was made (Apple: after 20, before 60
/// minutes).
const TOKEN_LIFE: Duration = Duration::from_secs(40 * 60);
/// A followed session's ordinary updates come at most this often.
const EVERY: Duration = Duration::from_secs(10);
/// Seconds after which a Live Activity shows as not updated, or its end is dismissed.
const STALE: u64 = 15 * 60;
/// An alert's open question ids, newest first, and the bytes of its whole payload.
const OPEN: usize = 50;
const PAYLOAD: usize = 4096;
/// A queued message expires after one day on either service.
const EXPIRE: u64 = 86_400;
/// The person is in a session while a client has its stream open and their last input to it is
/// this recent: a conversation's next message usually comes within a few minutes of the reply,
/// and a stream left open in an idle pane says nothing after that.
const HERE: Millis = 5 * 60 * 1000;

/// What the sender keeps of one status row.
struct Row {
    state: State,
    close_reason: Option<CloseReason>,
    parent: Option<SessionId>,
    name: String,
    headline: String,
    /// Its open questions' entries and times.
    asks: Vec<(EntryId, Millis)>,
    /// Its latest turn ended in an error, as read when it last stopped working.
    failed: bool,
    /// Its top-level turn finished and it has not worked since; a follow of it ends once none of
    /// its children works.
    finished: bool,
}

/// An alert's kind, its session and its entry; for `finished`, the devices that sent the
/// person's inputs its turn answered.
type Alert = (&'static str, SessionId, Option<EntryId>, Vec<String>);

/// A queued device registration, followed session (None for alerts), priority and payload.
struct Message(String, Push, Option<SessionId>, u8, Value);

mod fcm;

/// A follow's accepted content and next attempt, with its end kept until queued.
#[derive(Default)]
struct Shown {
    sent: Option<(Following, Instant)>,
    waiting: Option<Instant>,
    end: Option<Following>,
}

struct Sender {
    app: App,
    queue: mpsc::Sender<Message>,
    /// Exactly the status stream's rows; ordered, so a session's children come in one order.
    rows: BTreeMap<SessionId, Row>,
    /// By device, session and registration; at most the devices' 64 × 8 follows.
    shown: HashMap<(String, SessionId, Push), Shown>,
    /// No `agents` line has come yet.
    first: bool,
    /// Followed sessions read at the first `agents` line that had finished: each row is marked
    /// finished when it comes.
    ended: Vec<SessionId>,
}

/// Runs beside `serve` and never returns; it is dropped with the server.
pub(super) async fn send(app: App) {
    let client = reqwest::Client::builder().http2_prior_knowledge();
    let Ok(client) = client.timeout(Duration::from_secs(30)).build() else {
        eprintln!("razor: The push sender has no HTTP/2 client, so it sends nothing.");
        return std::future::pending().await;
    };
    let provider = Provider {
        client,
        tokens: Mutex::new(HashMap::new()),
        fcm: fcm::Fcm::default(),
    };
    let (queue, waiting) = mpsc::channel(WAITING);
    let posting = super::received(waiting).for_each_concurrent(AT_ONCE, |m| provider.post(&app, m));
    let mut changes = app.devices.changes();
    let (mut lines, mut deliveries) = (app.controller.status(), app.deliveries.subscribe());
    let mut sender = Sender {
        app: app.clone(),
        queue,
        rows: BTreeMap::new(),
        shown: HashMap::new(),
        first: true,
        ended: Vec::new(),
    };
    let reading = async move {
        loop {
            let due = sender.due();
            let wake = due.unwrap_or_else(Instant::now);
            tokio::select! {
                Some(line) = lines.recv() => sender.line(line).await,
                Ok(()) = changes.changed() => sender.follow().await,
                delivery = deliveries.recv() => match delivery {
                    Ok((d, true)) => {
                        let silent = matches!(d.sound, Some(crate::protocol::Sound::Terminal | crate::protocol::Sound::Silent));
                        let kind = if d.file.is_none() && d.url.is_none() { "notify" } else { "delivery" };
                        sender.alert((kind, d.session, Some(d.entry), Vec::new()), silent).await
                    }
                    Ok(_) | Err(RecvError::Lagged(_)) => {}
                    Err(RecvError::Closed) => return,
                },
                () = tokio::time::sleep_until(wake), if due.is_some() => sender.follow().await,
            }
        }
    };
    tokio::join!(posting, reading);
    std::future::pending().await
}

/// `{}` when this server can send to `environment`, else `{"unsent": "<why>"}`.
pub(super) async fn unsent(app: &App, registration: &Push) -> Value {
    let push = app.controller.config().await.push;
    let environment = registration.environment;
    let why = match registration.service {
        PushService::Fcm => match push.as_ref().and_then(|p| p.fcm_credentials.as_deref()) {
            Some(_) => None,
            None => Some("This server has no [push] fcm_credentials, so it sends this device nothing yet; the token is kept.".into()),
        },
        PushService::Apns => match push.as_ref().and_then(|p| Some((p, key(p, environment?)?))) {
            Some((push, key)) => sign(push, key).await.err().map(|e| e.to_string()),
            None => Some(format!(
                "This server's config has no [push] key for {} builds, so it sends this device nothing yet; the token is kept.",
                environment.map_or_else(|| "APNs".into(), |e| format!("{e:?}"))
            )),
        },
    };
    why.map_or_else(|| json!({}), |why| json!({ "unsent": why }))
}

fn key(push: &config::Push, environment: Environment) -> Option<&PushKey> {
    match environment {
        Environment::Development => push.development.as_ref(),
        Environment::Production => push.production.as_ref(),
    }
}

impl Sender {
    async fn line(&mut self, line: Live) {
        let mut alerts = Vec::new();
        let first = matches!(line, Live::Agents { .. }) && std::mem::take(&mut self.first);
        match line {
            Live::Agents { agents, .. } => {
                let mut before = std::mem::take(&mut self.rows);
                for view in agents {
                    let old = before.remove(&view.session);
                    self.row(old, view, &mut alerts).await;
                }
            }
            Live::Agent(view) => {
                let old = self.rows.remove(&view.session);
                self.row(old, *view, &mut alerts).await;
            }
            _ => return,
        }
        for alert in alerts {
            self.alert(alert, false).await;
        }
        if first {
            self.absent().await;
        }
        self.follow().await;
    }

    /// Stores `view`'s row and adds the alerts its change from `before` makes; a row first seen
    /// makes none.
    async fn row(&mut self, before: Option<Row>, view: AgentView, alerts: &mut Vec<Alert>) {
        let still = |kept: fn(&Row) -> bool| {
            before.as_ref().is_some_and(kept) && view.state != State::Working
        };
        let mut row = Row {
            state: view.state,
            close_reason: view.close_reason,
            parent: view.parent,
            name: view.name,
            headline: view.headline,
            asks: view.asks.iter().map(|a| (a.id.1, a.at)).collect(),
            failed: still(|r| r.failed),
            finished: still(|r| r.finished),
        };
        let session = view.session;
        let read = self.ended.len();
        self.ended.retain(|s| *s != session);
        row.finished |= self.ended.len() < read;
        if let Some(before) = before {
            let new = row
                .asks
                .iter()
                .filter(|(e, _)| before.asks.iter().all(|(b, _)| b != e));
            alerts.extend(new.map(|(e, _)| ("question", session.clone(), Some(*e), Vec::new())));
            let stopped = before.state == State::Working && row.state != State::Working;
            if stopped && row.parent.is_none() {
                let summary = self.app.controller.summary(&session).await;
                if let Ok(s) = summary
                    && !self.app.residents.timed(&s).await
                {
                    row.failed = matches!(s.last_end, Some(End::Error(_)));
                    // A stalled turn's last end is the turn before it, so stuck rules out done.
                    let stuck = row.state == State::Stuck || row.asks.is_empty() && row.failed;
                    let done = !stuck
                        && row.asks.is_empty()
                        && s.last_end == Some(End::Done)
                        && !s.idle_handover
                        && s.close_reason != Some(CloseReason::Idle);
                    let (noted, senders) = match done {
                        true => self.answered(&s).await,
                        false => (false, Vec::new()),
                    };
                    let kind =
                        (stuck.then_some("stuck")).or((done && !noted).then_some("finished"));
                    let alert = |kind| (kind, session.clone(), Some(s.last), senders);
                    alerts.extend(kind.map(alert));
                    row.finished = done;
                }
            }
        }
        self.rows.insert(session, row);
    }

    /// The turn that ended at `s.last`: whether the session noted something to the person during
    /// it, whose own push replaces the turn's finished alert, and the devices that sent the inputs
    /// it answered. Read from the end's input boundary, not the delivery channel: status may win
    /// that race.
    async fn answered(&self, s: &Summary) -> (bool, Vec<String>) {
        let session = &s.session;
        let end = async {
            let line = self.app.controller.get(session, s.last).await.ok()?;
            match serde_json::from_str::<Entry>(&line).ok()?.body {
                Body::TurnEnd { inputs, .. } => Some(inputs),
                _ => None,
            }
        };
        let (mut noted, mut senders) = (false, Vec::new());
        let Some(inputs) = end.await else {
            return (noted, senders);
        };
        if let Some(first) = inputs.iter().min()
            && let Ok(deliveries) = self.app.deliveries.list(None, 256, &[]).await
        {
            noted = deliveries.iter().any(|d| {
                d.session == *session
                    && d.file.is_none()
                    && d.url.is_none()
                    && d.entry > *first
                    && d.entry < s.last
            });
        }
        for input in inputs {
            if let Ok(line) = self.app.controller.get(session, input).await
                && let Ok(Entry {
                    body:
                        Body::Input(Input {
                            device: Some(d), ..
                        }),
                    ..
                }) = serde_json::from_str::<Entry>(&line)
            {
                senders.push(d);
            }
        }
        (noted, senders)
    }

    /// Queues the alert for each device with a token whose level and mutes let it through. Its
    /// open list is this server's open questions, cut, with `more` set, until the whole payload
    /// fits in 4 KB. A question is time-sensitive; anything else is passive for a device while the
    /// person is in the session there, else active.
    async fn alert(&self, (kind, session, entry, senders): Alert, silent: bool) {
        let top = self.top(&session);
        let watching = self.app.devices.watching(&session);
        let mut pushes = self.app.devices.pushes().await;
        pushes.retain(|(device, _, n)| {
            let watched = watching.contains(device);
            !n.muted.iter().any(|m| m == &session || m == top)
                && match (n.level, kind) {
                    (NotificationLevel::Off, _) => false,
                    (_, "question" | "stuck") => true,
                    (NotificationLevel::Questions, _) => false,
                    (NotificationLevel::Agents, "finished") => senders.contains(device) && !watched,
                    _ => true,
                }
        });
        if pushes.is_empty() {
            return;
        }
        let recent = kind != "question"
            && !watching.is_empty()
            && (self.app.controller.summary(&session).await).is_ok_and(|s| {
                (s.person_input_at).is_some_and(|at| now().saturating_sub(at) < HERE)
            });
        let label = super::label(&self.app.controller.config().await.server);
        let row = self.rows.get(&session).filter(|r| !r.name.is_empty());
        // An unnamed thread is named by its id, as the terminal shows it.
        let name = row.map_or_else(|| session.name(), |r| r.name.clone());
        let name: String = name.chars().take(64).collect();
        let said = match kind {
            "question" => "has a question",
            "delivery" => "delivered something",
            "notify" => "sent a note",
            "finished" => "finished",
            _ => "is stuck",
        };
        let asks = self
            .rows
            .iter()
            .map(|(s, r)| r.asks.iter().map(move |(e, at)| (*at, s, *e)));
        let mut open: Vec<_> = asks.flatten().collect();
        open.sort_by_key(|(at, ..)| std::cmp::Reverse(*at));
        let mut more = open.len() > OPEN;
        open.truncate(OPEN);
        let payload = loop {
            let ids: Vec<_> = open.iter().map(|(_, s, e)| (s, e)).collect();
            let mut payload = json!({
                "aps": {"alert": {"body": format!("{name} on {label} {said}")},
                        "mutable-content": 1, "category": kind, "thread-id": top,
                        "interruption-level": "time-sensitive"},
                "razor": {"server": &*self.app.id, "kind": kind, "session": session,
                          "entry": entry, "open": ids, "more": more},
            });
            if !silent {
                payload["aps"]["sound"] = json!("default");
            }
            if open.pop().is_none()
                || payload.to_string().len() <= PAYLOAD
                    && fcm::data(&payload, None).to_string().len() <= PAYLOAD
            {
                break payload;
            }
            more = true;
        };
        for (device, push, _) in pushes {
            let mut payload = payload.clone();
            let here = recent && watching.contains(&device);
            if kind != "question" {
                payload["aps"]["interruption-level"] =
                    json!(["active", "passive"][usize::from(here)]);
            }
            enqueue(&self.queue, Message(device, push, None, 10, payload));
        }
    }

    /// The top-level session `session` is under, by the rows' parents.
    fn top<'a>(&'a self, mut session: &'a SessionId) -> &'a SessionId {
        while let Some(parent) = self.rows.get(session).and_then(|r| r.parent.as_ref()) {
            session = parent;
        }
        session
    }

    /// Brings each current follow's Live Activity up to date, after a status line or when a
    /// waiting update is due: at once at priority 10 when its session gains a question or
    /// becomes needs-you or stuck, else at priority 5 no sooner than 10 seconds after the last,
    /// coalesced; its end when the session closes, or has finished and none of its children
    /// works.
    async fn follow(&mut self) {
        let follows = self.app.devices.follows().await;
        self.shown.retain(|(device, session, push), _| {
            follows
                .iter()
                .any(|(d, f)| d == device && f.session == *session && f.push == *push)
        });
        for (device, Follow { session, push }) in follows {
            let id = (device.clone(), session.clone(), push.clone());
            let ending = self.shown.get(&id).and_then(|s| s.end.clone());
            let (following, ended) = if let Some(following) = ending {
                (following, true)
            } else {
                let Some(row) = self
                    .rows
                    .get(&session)
                    .filter(|r| r.close_reason != Some(CloseReason::Idle))
                else {
                    self.shown.remove(&id);
                    continue;
                };
                let children = self
                    .rows
                    .values()
                    .filter(|r| r.parent.as_ref() == Some(&session));
                let working = children.clone().any(|r| r.state == State::Working);
                let children = children.filter(|r| r.state != State::Done);
                let following = Following {
                    state: row.state,
                    asks: row.asks.len(),
                    failed: row.failed,
                    headline: row.headline.chars().take(200).collect(),
                    children: children.take(32).map(|r| (r.state, r.asks.len())).collect(),
                };
                let ended = row.state == State::Done || row.finished && !working;
                (following, ended)
            };
            let shown = self.shown.entry(id.clone()).or_default();
            if ended {
                shown.end = Some(following.clone());
                shown.waiting = Some(Instant::now() + EVERY);
                let Ok(permit) = self.queue.try_reserve() else {
                    continue;
                };
                if let Err(e) = self
                    .app
                    .devices
                    .change(&device, |d| {
                        d.follows.retain(|f| f.session != session || f.push != push);
                    })
                    .await
                {
                    eprintln!("razor: {e}");
                    continue;
                }
                self.shown.remove(&id);
                permit.send(activity(device, push, session, &following, "end", 10));
                continue;
            }
            let last = shown.sent.as_ref().map(|(f, _)| f);
            if last == Some(&following) {
                shown.waiting = None;
                continue;
            }
            let urgent = last.is_some_and(|f| {
                let needs = matches!(following.state, State::NeedsYou | State::Stuck);
                f.asks < following.asks || f.state != following.state && needs
            });
            let due = (shown.sent.as_ref()).is_none_or(|(_, at)| at.elapsed() >= EVERY);
            if urgent || due {
                shown.waiting = Some(Instant::now() + EVERY);
                let Ok(permit) = self.queue.try_reserve() else {
                    continue;
                };
                let priority = if urgent { 10 } else { 5 };
                permit.send(activity(
                    device, push, session, &following, "update", priority,
                ));
                (shown.sent, shown.waiting) = (Some((following, Instant::now())), None);
            } else {
                shown.waiting = shown.sent.as_ref().map(|(_, at)| *at + EVERY);
            }
        }
    }

    /// At the first `agents` line, each followed session the stream does not hold is read, which
    /// puts its row on the stream; one that had finished or closed ends when its row comes (a
    /// closed one's row is done).
    async fn absent(&mut self) {
        for (_, Follow { session, .. }) in self.app.devices.follows().await {
            if !self.rows.contains_key(&session)
                && let Ok(s) = self.app.controller.summary(&session).await
                && !s.interrupted()
                && !s.idle_handover
                && s.close_reason != Some(CloseReason::Idle)
                && (s.closed || matches!(s.last_end, Some(End::Done)))
            {
                self.ended.push(session);
            }
        }
    }

    /// When the next waiting update may go.
    fn due(&self) -> Option<Instant> {
        self.shown.values().filter_map(|s| s.waiting).min()
    }
}

/// A Live Activity's `event`: an `update`, shown as not updated 15 minutes on, or its `end`,
/// dismissed then.
fn activity(
    device: String,
    push: Push,
    session: SessionId,
    following: &Following,
    event: &str,
    priority: u8,
) -> Message {
    let at = now() / 1000;
    let date = ["stale-date", "dismissal-date"][usize::from(event == "end")];
    let mut payload = json!({"aps": {"timestamp": at, "event": event, "content-state": following}});
    payload["aps"][date] = json!(at + STALE);
    Message(device, push, Some(session), priority, payload)
}

fn enqueue(queue: &mpsc::Sender<Message>, message: Message) {
    if queue.try_send(message).is_err() {
        eprintln!("razor: {WAITING} pushes are waiting; one more is dropped.");
    }
}

/// The HTTP/2 client and each environment's provider token: the team and key it was made with,
/// when, and the token.
struct Provider {
    client: reqwest::Client,
    tokens: Mutex<HashMap<Environment, (String, PushKey, Instant, String)>>,
    fcm: fcm::Fcm,
}

impl Provider {
    /// Posts through the registration's service; only an invalid registration is forgotten.
    async fn post(&self, app: &App, message: Message) {
        let Message(device, push, ..) = &message;
        let config = app.controller.config().await.push;
        let Some(config) = config.as_ref() else {
            return;
        };
        let request = match push.service {
            PushService::Apns => self.apple(config, &message).await,
            PushService::Fcm => self.fcm.request(&self.client, config, &message).await,
        };
        let request = match request {
            Ok(Some(request)) => request,
            Ok(None) => return,
            Err(e) => return eprintln!("razor: {e}"),
        };
        let answer = match request.send().await {
            Ok(answer) if answer.status().is_success() => return,
            Ok(answer) => answer,
            Err(e) => return eprintln!("razor: A push to {device} failed: {}.", e.without_url()),
        };
        let status = answer.status();
        let gone = match push.service {
            PushService::Apns => status == StatusCode::GONE,
            PushService::Fcm => fcm::gone(status, answer).await,
        };
        if gone && let Err(e) = app.devices.forget(device, push).await {
            eprintln!("razor: {e}");
        }
        eprintln!(
            "razor: {:?} refused a push to {device}: {status}.",
            push.service
        );
    }

    async fn apple(
        &self,
        config: &config::Push,
        Message(_, push, session, priority, payload): &Message,
    ) -> Result<Option<reqwest::RequestBuilder>, Error> {
        let Some((environment, key)) = push.environment.and_then(|e| Some((e, key(config, e)?)))
        else {
            return Ok(None);
        };
        let host = match (&config.endpoint, environment) {
            (Some(endpoint), _) => endpoint.as_str(),
            (None, Environment::Development) => "https://api.sandbox.push.apple.com",
            (None, Environment::Production) => "https://api.push.apple.com",
        };
        let (kind, topic) = match session {
            None => ("alert", config.bundle.clone()),
            Some(_) => (
                "liveactivity",
                format!("{}.push-type.liveactivity", config.bundle),
            ),
        };
        let token = self.token(config, key, environment).await?;
        Ok(Some(
            self.client
                .post(format!("{host}/3/device/{}", push.token))
                .header("authorization", format!("bearer {token}"))
                .header("apns-push-type", kind)
                .header("apns-topic", topic)
                .header("apns-priority", priority.to_string())
                .header("apns-expiration", (now() / 1000 + EXPIRE).to_string())
                .json(payload),
        ))
    }

    /// The environment's provider token, made again when the team or key changed (its id or
    /// its file) or it is 40 minutes old.
    async fn token(
        &self,
        push: &config::Push,
        key: &PushKey,
        environment: Environment,
    ) -> Result<String, Error> {
        let mut tokens = self.tokens.lock().await;
        if let Some((team, signer, made, token)) = tokens.get(&environment)
            && (team, signer) == (&push.team, key)
            && made.elapsed() < TOKEN_LIFE
        {
            return Ok(token.clone());
        }
        let token = sign(push, key).await?;
        let made = (
            push.team.clone(),
            key.clone(),
            Instant::now(),
            token.clone(),
        );
        tokens.insert(environment, made);
        Ok(token)
    }
}

/// A provider token: ES256 over Apple's header and claims, signed by the `.p8` key. An error
/// names the key's file, never its bytes.
async fn sign(push: &config::Push, key: &PushKey) -> Result<String, Error> {
    let file = key.key.display();
    let refused = |why: &str| Error::Refused(format!("The push key {file} {why}."));
    let pem = tokio::fs::read_to_string(&key.key).await;
    let pem = pem.map_err(|e| refused(&format!("does not read: {e}")))?;
    let der = pkcs8(&pem);
    let pair =
        der.and_then(|d| EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &d).ok());
    let pair = pair.ok_or_else(|| refused("is not an ES256 key; use the .p8 file Apple made"))?;
    let part = |value: Value| URL_SAFE_NO_PAD.encode(value.to_string());
    let head = part(json!({"alg": "ES256", "kid": key.key_id}));
    let claims = part(json!({"iss": push.team, "iat": now() / 1000}));
    let signed = format!("{head}.{claims}");
    let signature = pair.sign(&SystemRandom::new(), signed.as_bytes());
    let signature = signature.map_err(|_| refused("did not sign the provider token"))?;
    Ok(format!(
        "{signed}.{}",
        URL_SAFE_NO_PAD.encode(signature.as_ref())
    ))
}

/// Both services issue PEM-encoded PKCS#8 private keys.
fn pkcs8(pem: &str) -> Option<Vec<u8>> {
    let body: String = pem.lines().filter(|l| !l.starts_with("-----")).collect();
    STANDARD.decode(body.trim()).ok()
}
