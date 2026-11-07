//! Whether a model can run here: credential presence, shared by the new-session sheet, model
//! changes and doctor, and one tool-free answer on a model's own route, for recaps and doctor.

use crate::profiles::{self, Model, Transport};
use crate::{claude, config::Config, protocol::SessionId, responses::Responses};
use std::{collections::BTreeMap, path::Path, time::Duration};

/// Each check's refusal sentence; absent means Razor found the credentials or prerequisite.
/// Claude is checked with the given sign-in command, empty for Claude Code's own, within any deadline.
pub async fn credentials(
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
pub fn unready(
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
pub async fn default(
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
        None if claude => "the context window Claude Code reports".into(),
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
