//! Whether Razor can run here: `razor doctor`; credential presence, shared by the new-session
//! sheet, model changes and doctor; and one tool-free answer on a model's own route, for recaps
//! and doctor.

use super::{Error, KEY, LOCAL, SOCKET, client::answering};
use crate::config::{Config, setup};
use crate::profiles::{self, Model, Transport};
use crate::{claude, protocol::SessionId, residents, responses::Responses, tools::web};
use serde_json::Value;
use std::{collections::BTreeMap, io::IsTerminal, path::Path, time::Duration};

/// `razor doctor`: one line for each thing Razor runs on, found or missing with what to do, a
/// line for settings that do not load, and never a credential's value. It starts no server;
/// Claude Code may create its own config directory during the sign-in check. Ok when the settings
/// load and the default model can run, which on a provider takes a test request.
pub async fn doctor(home: &Path) -> Result<(), Error> {
    let config = Config::load(home).await;
    let sign_in = config.as_ref().map_or(&[][..], |c| &c.claude.sign_in);
    let checks = credentials(home, Some(sign_in), None).await;
    let fix = "For optional web search, run razor keys.";
    let keys = web::keys().map(|(name, set)| (name, (!set).then(|| fix.to_string())));
    let voice = std::env::var_os("ELEVENLABS_API_KEY").is_none();
    let voice = voice.then(|| "For optional voice, run razor keys.".into());
    for (name, why) in (checks.clone().into_iter())
        .chain(keys)
        .chain([("ELEVENLABS_API_KEY", voice)])
    {
        match why {
            None => println!("{name}: found."),
            Some(why) => println!("{name}: missing. {why}"),
        }
    }
    match &config {
        Err(e) => println!("Settings: invalid. {e} Edit the named file, then run razor doctor."),
        Ok(config) => println!("Pairing: {}", pairing(home, &config.server).await),
    }
    programs(home).await;
    let client = reqwest::Client::builder().unix_socket(home.join(SOCKET));
    let client = client.timeout(Duration::from_secs(2)).build().ok();
    if let Some(client) = &client
        && answering(home, client).await.is_some()
    {
        println!("Server: running.");
    } else if config.is_ok() {
        println!("Server: stopped; your next razor command starts it.");
    }
    let service = residents::check_service(home).await;
    match &service {
        Ok(true) => println!("Service: installed."),
        Ok(false) => println!("Service: absent (optional). Run razor install."),
        Err(e) => println!("Service: invalid. {e}"),
    }
    let service = service.is_ok_and(|s| s);
    if let Some(client) = &client
        && let Some(state) = permissions(home, client, service).await
    {
        println!("Mac permissions: {state}");
    }
    let fix = "Razor cannot start until its settings load; edit the file the Settings line names, then run razor doctor.";
    let config = config.map_err(|_| Error::Refused(fix.into()))?;
    let (said, runs) = default(home, &config, &checks).await;
    println!("Model: {said}");
    if !runs {
        return Err(Error::Refused(format!(
            "Razor cannot run on its default model, {}, yet; fix the Model line above, or choose another model with razor setup.",
            config.model
        )));
    }
    println!("Razor can run on {}.", config.model);
    Ok(())
}

/// Doctor's word on pairing: whether this server answers at `server.address` under its name.
async fn pairing(home: &Path, server: &crate::config::Server) -> String {
    let settings = home.join("config.toml");
    let settings = settings.display();
    let Some(address) = &server.address else {
        return format!(
            "no reachable server address; run razor setup with Tailscale running, or set address under [server] in {settings}."
        );
    };
    let listening = async {
        let probe = reqwest::Client::builder().no_proxy();
        let probe = probe.timeout(Duration::from_secs(2)).build().ok()?;
        let reply = probe
            .get(format!("http://{address}/v1/pair"))
            .send()
            .await
            .ok()?;
        let identity: Value = reply.error_for_status().ok()?.json().await.ok()?;
        Some(identity["name"] == server.name)
    };
    match listening.await {
        Some(true) => {
            format!("listening at {address} (reachability from other machines untested).")
        }
        _ => format!(
            "not listening at {address}; check Tailscale and address/name under [server] in {settings}, then run razor restart."
        ),
    }
}

/// Doctor's line for each program agents drive: the browser and the desktop daemon.
async fn programs(home: &Path) {
    let socket = home.join(residents::DESKTOP_SOCKET).into_os_string();
    for (name, program, args, found, fix) in [
        (
            "Browser",
            "agent-browser",
            ["--version".into()].to_vec(),
            "executable",
            "missing or unresponsive. Reinstall agent-browser, then run agent-browser install.",
        ),
        (
            "Desktop",
            "cua-driver",
            ["--socket".into(), socket, "status".into()].to_vec(),
            "daemon answering",
            "not running. The server runs cua-driver on a Mac where Razor has Accessibility, or on Linux with a display; server.log says why one stopped. Run razor restart.",
        ),
    ] {
        let probe = tokio::process::Command::new(setup::tool(program))
            .args(args)
            .kill_on_drop(true)
            .output();
        if tokio::time::timeout(Duration::from_secs(2), probe)
            .await
            .is_ok_and(|r| r.is_ok_and(|o| o.status.success()))
        {
            println!("{name}: found ({program} {found}; operation untested).");
        } else {
            println!("{name}: {fix}");
        }
    }
}

/// Doctor's word on the macOS permissions of the server for `home`, asked once through `client`,
/// which gives it 2 seconds; None off macOS or with no server running. With one missing, the
/// service's server running and a person at this terminal, it offers to have that server ask
/// macOS for both, which puts Razor in both lists; the answer defaults to no, as agents should
/// not see the screen unless the person wants it (docs/security.md).
async fn permissions(home: &Path, client: &reqwest::Client, service: bool) -> Option<&'static str> {
    let key = tokio::fs::read_to_string(home.join(KEY)).await.ok()?;
    let url = format!("{LOCAL}/v1/permissions");
    let unknown = "unknown: the server did not answer within 2 seconds. Run razor restart.";
    let held = match client.get(&url).bearer_auth(key.trim()).send().await {
        Err(e) if e.is_timeout() && cfg!(target_os = "macos") => return Some(unknown),
        answer => answer.ok()?.json::<Option<[bool; 2]>>().await.ok()??,
    };
    let terminal = std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
    Some(if held == [true; 2] {
        "found."
    } else if !service {
        "missing. To let agents see the screen and control this Mac, run razor install, then razor doctor in a terminal and answer yes."
    } else if !terminal
        || !setup::yes("Let agents see the screen and control this Mac?").unwrap_or(false)
    {
        "missing. To let agents see the screen and control this Mac, run razor doctor in a terminal and answer yes."
    } else {
        let post = client.post(&url).bearer_auth(key.trim()).send();
        match async { post.await?.json::<Option<[bool; 2]>>().await }.await {
            Ok(Some(_)) => {
                "requested. Turn on Razor in System Settings, Privacy & Security, under Accessibility (Device Control and Data Access on macOS 27) and Screen & System Audio Recording; then run razor restart."
            }
            Ok(None) => {
                "missing. Razor's service did not start this server; run razor restart, then razor doctor."
            }
            Err(_) => unknown,
        }
    })
}

/// Each check's refusal sentence; absent means Razor found the credentials or prerequisite.
/// Claude is checked with the given sign-in command, empty for Claude Code's own, within any deadline.
pub(crate) async fn credentials(
    home: &Path,
    sign_in: Option<&[String]>,
    deadline: Option<Duration>,
) -> BTreeMap<&'static str, Option<String>> {
    let mut checks = BTreeMap::new();
    let found = tokio::fs::try_exists(home.join("auth.json")).await;
    let missing = crate::responses::SIGN_IN_MISSING.to_owned();
    checks.insert(
        "ChatGPT sign-in",
        (!found.unwrap_or(false)).then_some(missing),
    );
    if let Some(command) = sign_in {
        let check = async {
            let dir = claude::dir()
                .await
                .map_err(|e| format!("{e} Run razor setup."))?;
            let node = claude::node().await;
            let version = tokio::process::Command::new(&node)
                .arg("--version")
                .kill_on_drop(true)
                .output()
                .await;
            if !version.is_ok_and(|o| o.status.success()) {
                let missing = if node == Path::new("node") {
                    "this install has no private Node; "
                } else {
                    ""
                };
                return Err(format!(
                    "The Claude bridge needs Node 22.18 or later on PATH ({}); {missing}install Node, then run razor doctor.",
                    std::env::var("PATH").unwrap_or_default()
                ));
            }
            if !command.is_empty() {
                let signed = claude::shared::env(command, &dir, "").await;
                return signed.map(drop).map_err(|e| {
                    format!("{e} Check claude.sign_in in your settings, then run razor doctor.")
                });
            }
            let config = std::env::var_os("CLAUDE_CONFIG_DIR")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| {
                    std::path::PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
                        .join(".claude")
                });
            claude::sign_in(&dir)
                .await
                .map(drop)
                .map_err(|e| format!("Claude sign-in using {}: {e}", config.display()))
        };
        // A check that outlasts the deadline is no evidence of a missing sign-in: on a busy
        // machine the CLI alone can take longer, so the request goes ahead and its turn reports.
        let result = match deadline {
            Some(deadline) => tokio::time::timeout(deadline, check)
                .await
                .unwrap_or(Ok(())),
            None => check.await,
        };
        checks.insert("Claude", result.err());
    }
    checks
}

/// Why `model` cannot run here, checking only its own route's credentials within two seconds;
/// `None` when Razor found them.
pub(super) async fn model(home: &Path, config: &Config, model: &str) -> Option<String> {
    let route = profiles::model(model, config).map(|m| m.endpoint);
    let signed = route.is_ok_and(|e| e.transport == Transport::Claude && e.key.is_none());
    let sign_in = (signed && config.claude.sign_in.is_empty()).then_some(&[][..]);
    let checks = credentials(home, sign_in, Some(Duration::from_secs(2))).await;
    unready(&checks, config, model)
}

/// An absent reason means Razor found the credentials or a provider's key;
/// entitlement, quota and network are untried.
pub(super) fn unready(
    checks: &BTreeMap<&str, Option<String>>,
    config: &Config,
    model: &str,
) -> Option<String> {
    let route = match profiles::model(model, config) {
        Ok(route) => route.endpoint,
        Err(e) => return Some(e.to_string()),
    };
    let name = match (route.transport, route.key) {
        (_, Some(key)) => {
            return (!key.is_empty() && std::env::var_os(&key).is_none()).then(|| unset(&key));
        }
        (Transport::Claude, None) if !config.claude.sign_in.is_empty() => return None,
        (Transport::Claude, None) => "Claude",
        (_, None) => "ChatGPT sign-in",
    };
    checks.get(name).cloned().flatten()
}

fn unset(key: &str) -> String {
    format!("{key} is not set; save it with razor setup or in keys.toml, then run razor restart.")
}

/// One tool-free answer from `route`'s model to `text` under `prompt`, in `cwd`: through Claude
/// Code on a Claude route, with a provider's variables when it has a key, else on Razor's own
/// loop. A failure is one sentence naming what to check.
pub(super) async fn answer(
    home: &Path,
    responses: &Responses,
    cwd: &Path,
    route: &Model,
    session: &SessionId,
    prompt: &str,
    text: &str,
) -> Result<String, String> {
    use crate::responses::Error::{Failed, Incomplete, Other, Rejected};
    let endpoint = &route.endpoint;
    let provider = route.provider.as_deref().unwrap_or_default();
    let at = format!("[providers.{provider}]");
    if endpoint.transport != Transport::Claude {
        let profile = profiles::load(route.clone(), false, None, Vec::new().into(), false, false);
        let mut profile = profile.map_err(|e| e.to_string())?;
        profile.prompt = prompt.into();
        let answered = responses.answer(&profile, session, text).await;
        return answered.map_err(|e| {
            let check = match e {
                Rejected { status: 404, .. } => "base_url (it usually ends in /v1) and format",
                Rejected { .. } => "the model id and format",
                Other { .. } => "that this machine reaches base_url, and base_url",
                Incomplete | Failed(_) => "format: chat, responses or anthropic",
                _ => return e.to_string(),
            };
            format!("{e} Check {check} under {at}.")
        });
    }
    let env = match endpoint.key {
        Some(_) => claude::provider(endpoint, &home.join("claude")).map_err(|e| e.to_string())?,
        None => serde_json::Value::Null,
    };
    let answered = claude::answer(cwd, &endpoint.id, &env, prompt, text).await;
    answered.map_err(|e| format!("{e} Check the key, base_url and model id under {at}."))
}

/// Doctor's word on the default model: its route (provider, address, model id, where its key came
/// from but never its value, its context window), then on a provider one test request; true when
/// it can run.
async fn default(
    home: &Path,
    config: &Config,
    checks: &BTreeMap<&str, Option<String>>,
) -> (String, bool) {
    let name = &config.model;
    let route = match profiles::model(name, config) {
        Ok(route) => route,
        Err(e) => return (format!("{name} cannot run: {e}"), false),
    };
    let claude = route.endpoint.transport == Transport::Claude;
    let window = match profiles::limits(name, config, None).0 {
        Some(window) => format!("a context window of {window} tokens"),
        None if claude => format!(
            "the context window Claude Code reports, at most {} tokens",
            profiles::LARGE.0
        ),
        None => format!("an unknown context window, so set window under [models.\"{name}\"]"),
    };
    let (Some(key), Some(provider)) = (&route.endpoint.key, &route.provider) else {
        let signed = [
            ("ChatGPT sign-in", "Razor's ChatGPT sign-in"),
            ("Claude", "Claude's sign-in"),
        ];
        let (check, on) = signed[usize::from(claude)];
        let why = checks.get(check).cloned().flatten();
        return match why {
            None => (format!("{name}, on {on}, with {window}."), true),
            Some(why) => (format!("{name} cannot run on {on}: {why}"), false),
        };
    };
    let format = serde_json::to_value(route.endpoint.transport).unwrap_or_default();
    let (url, id, format) = (&route.endpoint.url, &route.endpoint.id, format.as_str());
    let key = match crate::config::setup::source(home, key) {
        _ if key.is_empty() => "no key".into(),
        Some(source) => format!("{key} from {source}"),
        None => return (format!("{name} cannot run: {}", unset(key)), false),
    };
    let format = format.unwrap_or_default();
    let said = format!("{name}: {id} on {provider}, the {format} endpoint {url}, {key}, {window}");
    let (responses, session) = (Responses::new(home.to_path_buf()), SessionId::new());
    let responses = match responses {
        Ok(responses) => responses,
        Err(e) => return (format!("{said}; {e}"), false),
    };
    let (prompt, text) = ("Reply with OK.", "OK?");
    let ask = answer(home, &responses, home, &route, &session, prompt, text);
    match tokio::time::timeout(Duration::from_secs(60), ask).await {
        Ok(Ok(_)) => (format!("{said}; a test request was answered."), true),
        Ok(Err(why)) => (format!("{said}; the test request failed. {why}"), false),
        Err(_) => (format!("{said}; no answer came in 60 seconds."), false),
    }
}
