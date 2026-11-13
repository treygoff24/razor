//! The client half: how a command reaches this home's server, starting it when none answers, or
//! `[remote]`'s; and the commands that only talk to it.

use super::{Error, KEY, LOCAL, SOCKET, other};
use crate::protocol::{Body, ClientInput, End, EntryId, Event, Login, SessionId, VoiceSettings};
use crate::residents;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::os::unix::{fs::OpenOptionsExt, process::CommandExt};
use std::path::{Path, PathBuf};
use std::process::{Child, ExitStatus, Stdio};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio::sync::mpsc;

/// A local socket client and the default paired destination for session commands.
pub struct Connection {
    pub http: reqwest::Client,
    pub url: reqwest::Url,
    pub key: String,
    pub cwd: PathBuf,
    pub server: String,
}

/// Session commands use the local socket; a configured pair is reached by one-hop forwarding.
pub async fn connection(home: &Path) -> Result<Connection, Error> {
    // Settings that do not load leave the local server to start, fail and name why in its log.
    let config = crate::config::Config::load(home).await.ok();
    let remote = config.as_ref().and_then(|c| c.remote.as_ref());
    let server = remote.map(|r| r.server.clone()).unwrap_or_default();
    if remote.is_some() && !crate::devices::valid_name(&server) {
        return Err(Error::Refused("[remote] names a paired server, as server = \"<its name in razor devices>\", and holds no address or key (docs/machines.md).".into()));
    }
    if !server.is_empty() && config.as_ref().is_some_and(|c| c.provider_only) {
        return Err(Error::Refused(
            "provider_only is on; remove [remote] or set provider_only = false.".into(),
        ));
    }
    let unreadable =
        "The working directory could not be read; start Razor in an existing directory.";
    let cwd = remote.and_then(|r| r.folder.clone()).map_or_else(
        || std::env::current_dir().map_err(|_| Error::Refused(unreadable.into())),
        Ok,
    )?;
    let (http, key) = connect(home).await?;
    let url = reqwest::Url::parse(LOCAL).map_err(other("Making the local server address"))?;
    Ok(Connection {
        http,
        url,
        key,
        cwd,
        server,
    })
}

/// A client and local key for `home`. Start its installed service, or a detached server,
/// when none answers, then wait up to ten seconds for readiness.
pub async fn connect(home: &Path) -> Result<(reqwest::Client, String), Error> {
    let client = client(reqwest::Client::builder().unix_socket(home.join(SOCKET)))?;
    if let Some(key) = answering(home, &client).await {
        return Ok((client, key));
    }
    let failed = |e: residents::Error| Error::Refused(e.to_string());
    crate::config::make_home(home).map_err(|e| Error::Refused(e.to_string()))?;
    let service = residents::start(home).await.map_err(failed)?;
    let log = home.join("server.log");
    let child = if service {
        None
    } else {
        Some(spawn(&log).await?)
    };
    let probe = async |mut child: Option<Child>| {
        let mut delay = Duration::from_millis(1);
        loop {
            if let Some(key) = answering(home, &client).await {
                return Ok::<_, Error>(Ok(key));
            }
            if let Some(child) = &mut child
                && let Some(status) = child
                    .try_wait()
                    .map_err(|e| Error::Refused(format!("Checking razor serve failed: {e}.")))?
            {
                if let Some(key) = answering(home, &client).await {
                    return Ok(Ok(key));
                }
                if !residents::server_running(home).map_err(failed)? {
                    return Ok(Err(status));
                }
            }
            tokio::time::sleep(delay).await;
            delay = (delay * 2).min(Duration::from_millis(100));
        }
    };
    let ready = async {
        if let Ok(key) = probe(child).await? {
            return Ok(key);
        }
        // A lock holder can exit after our child loses the race; make a server exist once more.
        match probe(Some(spawn(&log).await?)).await? {
            Ok(key) => Ok(key),
            Err(status) => Err(exited(&log, status).await),
        }
    };
    if let Ok(result) = tokio::time::timeout(Duration::from_secs(10), ready).await {
        return result.map(|key| (client, key));
    }
    let logs = if service && cfg!(target_os = "linux") {
        let unit = residents::installed(home).await.map_err(failed)?;
        unit.map_or_else(
            || "journalctl --user".to_owned(),
            |(_, label)| format!("journalctl --user -u {label}.service"),
        )
    } else {
        log.display().to_string()
    };
    Err(Error::Refused(format!(
        "No Razor server answered within 10 seconds; {logs} says why."
    )))
}

fn client(builder: reqwest::ClientBuilder) -> Result<reqwest::Client, Error> {
    let built = builder.build();
    built.map_err(|e| Error::Refused(format!("Razor cannot make its client: {e}.")))
}

/// The local key, if a server for `home` answers to it through its socket.
pub(super) async fn answering(home: &Path, client: &reqwest::Client) -> Option<String> {
    let key = tokio::fs::read_to_string(home.join(KEY)).await.ok()?;
    let key = key.trim().to_string();
    let probe = client.get(format!("{LOCAL}/v1/devices"));
    let answer = probe.bearer_auth(&key).send().await.ok()?;
    answer.status().is_success().then_some(key)
}

/// Start a detached server with both output streams in its private log.
async fn spawn(log: &Path) -> Result<Child, Error> {
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(log)
        .map_err(|e| Error::Refused(format!("server.log cannot be opened: {e}.")))?;
    let exe =
        (residents::selected_executable().await).map_err(|e| Error::Refused(e.to_string()))?;
    let started = (|| {
        let errors = file.try_clone()?;
        std::process::Command::new(exe)
            .arg("serve")
            .stdin(Stdio::null())
            .stdout(file)
            .stderr(errors)
            .process_group(0)
            .spawn()
    })();
    started.map_err(|e| Error::Refused(format!("razor serve cannot start: {e}.")))
}

/// Report an exited server's last log line, reading at most 4 KiB even after many starts.
async fn exited(log: &Path, status: ExitStatus) -> Error {
    let tail = async {
        let mut file = tokio::fs::File::open(log).await?;
        let start = file.metadata().await?.len().saturating_sub(4096);
        file.seek(std::io::SeekFrom::Start(start)).await?;
        let mut bytes = Vec::new();
        file.take(4096).read_to_end(&mut bytes).await?;
        Ok::<_, std::io::Error>(bytes)
    };
    let bytes = tail.await.unwrap_or_default();
    let text = String::from_utf8_lossy(&bytes);
    let cause = text.lines().rev().find(|line| !line.trim().is_empty());
    let cause = cause.map_or(String::new(), |line| format!("; last line: {line:?}"));
    Error::Refused(format!(
        "razor serve exited ({status}) before answering; see {}{cause}.",
        log.display()
    ))
}

/// Send a client request to the server; a refusal becomes its sentence.
pub(super) async fn send(request: reqwest::RequestBuilder) -> Result<reqwest::Response, Error> {
    let answer = (request.send().await).map_err(other("Reaching the Razor server"))?;
    if answer.status().is_success() {
        return Ok(answer);
    }
    let refusal: Value = answer.json().await.unwrap_or_default();
    let sentence = refusal["error"]
        .as_str()
        .unwrap_or("The server refused the request.");
    Err(Error::Refused(sentence.into()))
}

/// The server's JSON answer to `request`, or its sentence for a refusal.
pub(super) async fn json<T: DeserializeOwned>(
    request: reqwest::RequestBuilder,
) -> Result<T, Error> {
    let answer = send(request).await?;
    answer
        .json()
        .await
        .map_err(other("Reading the server's answer"))
}

/// The next line of a streamed answer, its newline included; None at the answer's end. Between
/// calls, `pending` holds the part of the answer already read and not yet returned.
async fn line(
    answer: &mut reqwest::Response,
    pending: &mut Vec<u8>,
) -> Result<Option<Vec<u8>>, reqwest::Error> {
    loop {
        if let Some(newline) = pending.iter().position(|b| *b == b'\n') {
            return Ok(Some(pending.drain(..=newline).collect()));
        }
        let Some(chunk) = answer.chunk().await? else {
            return Ok(None);
        };
        pending.extend_from_slice(&chunk);
    }
}

/// `--continue`: the newest session other than a dream in the connection's folder.
pub async fn newest(c: &Connection) -> Result<SessionId, Error> {
    let cwd = c.cwd.to_string_lossy();
    let listed = c
        .http
        .get(format!("{}v1/sessions", c.url))
        .query(&[("skip_dreams", true)])
        .query(&[("server", &c.server)])
        .bearer_auth(&c.key);
    let listed: Value = json(listed.query(&[("cwd", cwd.as_ref()), ("limit", "1")])).await?;
    let none = || Error::Refused("There is no session in this directory to continue.".into());
    session(listed[0]["session"].as_str().ok_or_else(none)?).map(|s| s.on(&c.server))
}

fn session(id: &str) -> Result<SessionId, Error> {
    SessionId::try_from(id.to_owned()).map_err(|e| Error::Refused(e.to_string()))
}

/// `--print`'s turn: `input` admitted to `session`, or to a new session in the connection's folder
/// on the input's model, effort and speed; then the session's stream read until the turn that
/// answers it ends, each line from the input on copied to standard output with `stream`. The
/// session, how the turn ended, and the turn's text after its last call.
pub async fn turn(
    c: &Connection,
    session: Option<SessionId>,
    input: &ClientInput,
    stream: bool,
) -> Result<(SessionId, End, String), Error> {
    let post = |path: &str, body: Value| {
        let request = c.http.post(format!("{}{path}", c.url));
        request.bearer_auth(&c.key).json(&body)
    };
    let session = session.map(|s| s.on(&c.server));
    let (model, effort, speed) = (&input.model, input.effort, input.speed);
    let session = match session {
        Some(session) => session,
        None => {
            let body = json!({ "cwd": c.cwd, "model": model, "effort": effort, "speed": speed });
            let created: Value =
                json(post(&format!("v1/sessions?server={}", c.server), body)).await?;
            self::session(created["session"].as_str().unwrap_or_default())?.on(&c.server)
        }
    };
    // Subscribe before admitting input. A cursor beyond the record skips old turns while
    // retaining every live line from the subscription onward.
    let url = format!("{}v1/sessions/{session}/events?after={}", c.url, u64::MAX);
    let mut events = send(c.http.get(url).bearer_auth(&c.key)).await?;
    let body = json!({ "text": input.text, "model": model, "effort": effort, "speed": speed });
    let admitted: Value = json(post(&format!("v1/sessions/{session}/input"), body)).await?;
    let input = EntryId(admitted["entry"].as_u64().unwrap_or(0));
    let gone = || Error::Refused("The Razor server went away before the turn ended.".into());
    // `texts` holds the turn's text since its last call.
    let (mut pending, mut texts) = (Vec::new(), Vec::<String>::new());
    let (mut stdout, mut started) = (tokio::io::stdout(), false);
    while let Some(line) = line(&mut events, &mut pending).await.map_err(|_| gone())? {
        let Ok(event) = serde_json::from_slice::<Value>(&line) else {
            continue;
        };
        started |= event["id"].as_u64().is_some_and(|id| id >= input.0);
        // A working session may send lines between subscription and our input.
        if !started && event["kind"] != "attached" {
            continue;
        }
        if stream {
            let _ = stdout.write_all(&line).await;
            let _ = stdout.flush().await;
        }
        let Ok(Event::Entry(entry)) = serde_json::from_value::<Event>(event) else {
            continue;
        };
        match entry.body {
            Body::Text { text } => texts.push(text),
            Body::Call { .. } => texts.clear(),
            Body::TurnEnd { inputs, end, .. } if inputs.contains(&input) => {
                return Ok((session, end, crate::record::closing_text(&texts)));
            }
            Body::TurnEnd { .. } => texts.clear(),
            _ => {}
        }
    }
    Err(gone())
}

/// `razor login`: the selected provider runs in the server; codes never enter a session.
pub async fn login(
    client: &reqwest::Client,
    key: &str,
    provider: crate::protocol::LoginProvider,
) -> Result<(), Error> {
    let url = format!("{LOCAL}/v1/login");
    let mut answer = send(
        client
            .post(&url)
            .query(&[("provider", provider)])
            .bearer_auth(key),
    )
    .await?;
    let claude = provider != crate::protocol::LoginProvider::Chatgpt;
    let (typed, mut codes) = mpsc::channel(1);
    if claude {
        println!("After signing in at the link below, paste the code here and press Enter.");
        // A detached stdin reader lets the command exit when a browser callback completes.
        std::thread::spawn(move || {
            use std::io::BufRead;
            let stdin = std::io::stdin();
            for code in stdin.lock().lines().map_while(Result::ok) {
                if typed.blocking_send(code).is_err() {
                    break;
                }
            }
        });
    }
    let mut pending = Vec::new();
    let reading = "Reading the sign-in";
    loop {
        let line = tokio::select! {
            line = line(&mut answer, &mut pending) => line.map_err(other(reading))?,
            Some(code) = codes.recv(), if claude => {
                if code.trim().is_empty() {
                    continue;
                }
                let body = json!({"code": code});
                if let Err(e) = send(client.put(&url).bearer_auth(key).json(&body)).await {
                    eprintln!("{e}");
                }
                continue;
            }
        };
        let Some(line) = line else { break };
        match serde_json::from_slice(&line).map_err(other(reading))? {
            Login::Show { text } => println!("{text}"),
            Login::Done => return Ok(()),
            Login::Failed { error } => return Err(Error::Refused(error)),
        }
    }
    Err(Error::Refused(
        "The Razor server went away before the sign-in ended.".into(),
    ))
}

/// `razor config get|set|unset voice.<field>`: one voice setting on this home's server, after
/// any change. A set value is JSON, else text; unset restores the default. The day's counts are
/// the server's own, so only get takes them.
pub async fn voice(client: &reqwest::Client, key: &str, words: &[&str]) -> Result<String, Error> {
    let field = words[1].strip_prefix("voice.").unwrap_or(words[1]);
    let (none, mut every) = (VoiceSettings::default(), VoiceSettings::default());
    (every.daily_characters, every.call_daily_dollars) = (Some(0), Some(0.0));
    if json!(every).get(field).is_none() || (words[0] != "get" && field.ends_with("used_today")) {
        let (verb, see) = (words[0], "see browser-voice-search.md");
        let why = format!("voice.{field} is not a voice setting you can {verb}; {see}.");
        return Err(Error::Refused(why));
    }
    let (url, value) = (format!("{LOCAL}/v1/voice"), words.get(2).copied());
    let request = match value.map(|v| serde_json::from_str(v).unwrap_or(json!(v))) {
        Some(value) => client.put(url).json(&json!({ field: value })),
        None if words[0] == "unset" => client.put(url).json(&json!({ field: json!(none)[field] })),
        None => client.get(url),
    };
    let settings: Value = json(request.bearer_auth(key)).await?;
    Ok(format!("voice.{field}: {}", settings[field]))
}

/// `razor devices`: the enrolled devices' names, one a line.
pub async fn devices(client: &reqwest::Client, key: &str) -> Result<String, Error> {
    let listed: Vec<Value> =
        json(client.get(format!("{LOCAL}/v1/devices")).bearer_auth(key)).await?;
    if listed.is_empty() {
        return Ok("No device is enrolled; razor devices add <name> enrolls one.".into());
    }
    let names = listed.iter().filter_map(|d| d["name"].as_str());
    Ok(names.collect::<Vec<_>>().join("\n"))
}

/// `razor devices add <name> --key <key>`: a program that sends `given` on this machine's socket.
pub async fn enroll(
    client: &reqwest::Client,
    key: &str,
    name: &str,
    given: &str,
) -> Result<String, Error> {
    let request = client.post(format!("{LOCAL}/v1/devices")).bearer_auth(key);
    json::<Value>(request.json(&json!({ "name": name, "key": given }))).await?;
    Ok(format!(
        "{name} is enrolled; its key works on the local socket."
    ))
}

/// How `razor devices` is used.
pub const DEVICES_USE: &str = "Use razor login, razor devices, razor devices add <name> (it shows a code; the other server runs razor devices join <name> and pastes it), razor devices add <name> --key <key> for a program on this machine, or razor devices remove <name>.";

/// `razor devices remove <name>`: what to print once the key no longer works.
pub async fn revoke(client: &reqwest::Client, key: &str, name: &str) -> Result<String, Error> {
    let request = client.delete(format!("{LOCAL}/v1/devices/{name}"));
    send(request.bearer_auth(key)).await?;
    Ok(format!("{name} is removed; its key no longer works."))
}
