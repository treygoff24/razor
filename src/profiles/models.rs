//! Model names, providers and the route each name takes under the live settings.

use super::{Endpoint, Error, Replay};
use crate::config::Config;
use crate::protocol::{Effort, Speed, SpeedChoice};
use serde::{Deserialize, Serialize};

/// The wire a model is reached over, which is also a provider's `format`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Transport {
    /// The Claude Agent SDK, speaking Anthropic's Messages API.
    #[serde(rename = "anthropic")]
    Claude,
    Responses,
    /// OpenAI-compatible `POST {base_url}/chat/completions`, streamed (docs/design/generic.md).
    Chat,
}

/// A shipped chat model's row: its provider and its id there, its context window, the highest
/// effort it takes (a higher one is sent as this), which field earlier reasoning goes back under
/// as its probe proved it, and the variant it gets when config names none, its leader in the
/// evaluation. `[models."<model>"]` replaces its window, replay and variant.
struct Chat {
    provider: &'static str,
    id: &'static str,
    window: u64,
    top: Effort,
    replay: Replay,
    variant: &'static str,
}

/// A model's route under the live settings: where its requests go, the provider it is on (none
/// for Claude's and GPT's own sign-ins), its window when config or the shipped table knows it, and on
/// chat the variant it gets when config names none.
#[derive(Clone)]
pub(crate) struct Model {
    pub(crate) endpoint: Endpoint,
    pub(crate) provider: Option<String>,
    pub(super) window: Option<u64>,
    pub(super) variant: &'static str,
}

impl Model {
    /// Offered efforts in ascending order, capped by a shipped chat row's top.
    pub(crate) fn efforts(&self) -> &'static [Effort] {
        let top = self.endpoint.top;
        &Effort::ALL[..Effort::ALL.partition_point(|effort| *effort <= top)]
    }
}

/// The route `name` takes: Claude's names on the SDK with its own sign-in, GPT's on Razor's
/// ChatGPT sign-in at `[responses] base_url`, a shipped chat name on its provider, and
/// `<provider>/<id>` on that provider. With `provider_only` only a provider named under
/// `[providers]` is allowed; a refusal says so.
pub(crate) fn model(name: &str, config: &Config) -> Result<Model, Error> {
    let route = route(name, config)?;
    if config.provider_only
        && !(route.provider.as_ref()).is_some_and(|p| config.providers.contains_key(p))
    {
        return Err(Error::Invalid(format!(
            "provider_only is on, so Razor runs only models on a provider under [providers] in your settings, and {name} is on none; add its provider there (docs/models.md#providers) or set provider_only = false"
        )));
    }
    Ok(route)
}

/// `model` without the `provider_only` check: what the name stands for.
fn route(name: &str, config: &Config) -> Result<Model, Error> {
    let choice = id(name.into());
    let row = settings(&choice, config);
    let name = resolved(&choice);
    let chat = CHAT.iter().find(|(n, _)| *n == name).map(|(_, chat)| chat);
    let mut endpoint = Endpoint {
        transport: Transport::Claude,
        url: String::new(),
        key: None,
        id: name.into(),
        top: chat.map_or(Effort::Max, |c| c.top),
        speed_ids: row.map(|r| r.speed_ids.clone()).unwrap_or_default(),
        replay: (row.and_then(|r| r.replay)).unwrap_or(chat.map_or(Replay::Streamed, |c| c.replay)),
        request: row.and_then(|r| r.request.clone()).unwrap_or_default(),
    };
    let (provider, id) = match (chat, name.split_once('/')) {
        (Some(chat), _) => (Some(chat.provider), chat.id),
        (None, Some((provider, id))) if !id.is_empty() => (Some(provider), id),
        _ if name.starts_with("claude-") => (None, name),
        _ if window(name).is_some() => {
            endpoint.transport = Transport::Responses;
            endpoint.url.clone_from(&config.responses.base_url);
            (None, name)
        }
        _ => {
            let names = FAMILY
                .iter()
                .map(|(name, ..)| *name)
                .collect::<Vec<_>>()
                .join(", ");
            return Err(Error::Invalid(format!(
                "{name} is neither a model Razor ships nor a provider's model; check the name ({names}, or an exact model id), or name it <provider>/<id> with the provider under [providers]"
            )));
        }
    };
    if let Some(provider) = provider {
        let table = config.providers.get(provider);
        let ship = PROVIDERS.iter().find(|(name, ..)| *name == provider);
        let url = table
            .and_then(|t| t.base_url.clone())
            .or(ship.map(|s| s.2.into()));
        endpoint.url = url.ok_or_else(|| {
            Error::Invalid(format!(
                "{name} is on the provider {provider}, which needs a [providers.{provider}] table with its base_url"
            ))
        })?;
        let format = table.and_then(|t| t.format).or(ship.map(|s| s.1));
        endpoint.transport = format.unwrap_or(Transport::Chat);
        let key = table
            .and_then(|t| t.key.clone())
            .or(ship.map(|s| s.3.into()));
        (endpoint.key, endpoint.id) = (Some(key.unwrap_or_default()), id.into());
    }
    Ok(Model {
        endpoint,
        provider: provider.map(Into::into),
        window: chat.map(|c| c.window).or(window(id)),
        variant: chat.map_or("{}", |c| c.variant),
    })
}

/// Refuses invalid limits and request settings that cannot apply to `name`'s route.
pub(crate) fn validate(
    name: &str,
    row: &crate::config::Model,
    config: &Config,
) -> Result<(), Error> {
    if row.window == Some(0)
        || row
            .threshold
            .is_some_and(|t| !t.is_finite() || !(0.0 < t && t <= 1.0))
    {
        return Err(Error::Invalid(format!(
            "[models.\"{name}\"] needs a positive window and a threshold greater than zero and at most one"
        )));
    }
    if route(name, config)?.endpoint.transport != Transport::Chat
        && (row.request.is_some()
            || row.variant.is_some()
            || !row.speed_ids.is_empty()
            || row.replay.is_some())
    {
        return Err(Error::Invalid(format!(
            "[models.\"{name}\"] is not a chat model; remove request, variant, replay and speed_ids"
        )));
    }
    Ok(())
}

/// Resolves Razor's limits from live settings, else the shipped or SDK window at most
/// `LARGE`'s, handed over at 0.8 or, at that size, `LARGE`'s share; a configured Claude window
/// never changes what its SDK sends.
pub(crate) fn limits(name: &str, config: &Config, reported: Option<u64>) -> (Option<u64>, f64) {
    let row = settings(name, config);
    let window = row.and_then(|r| r.window).or_else(|| {
        let shipped = route(name, config).ok().and_then(|m| m.window);
        Some(shipped.or(reported.filter(|w| *w > 0))?.min(LARGE.0))
    });
    let share = [0.8, LARGE.1][usize::from(window >= Some(LARGE.0))];
    let threshold = row.and_then(|r| r.threshold).or(config.rebuild.threshold);
    (window, threshold.unwrap_or(share))
}

/// The most context Razor fills on a model without a configured window, and the share of it
/// handed over at unless a threshold is set. Trey, 6 Nov 2026: "500k context window for all
/// models that support it that high, auto handover/compaction around 425 or 450".
pub(crate) const LARGE: (u64, f64) = (500_000, 0.9);

/// GPT's shipped defaults by id; config overrides them.
fn window(id: &str) -> Option<u64> {
    MODELS.iter().find(|(n, _)| *n == id).map(|(_, w)| *w)
}

const MODELS: &[(&str, u64)] = &[
    ("gpt-6-astra", 272_000),
    ("gpt-6.1-sol", 500_000),
    ("gpt-6-sol", 272_000),
    ("gpt-6-luna", 272_000),
    ("gpt-5.6-sol", 272_000),
    ("gpt-5.6-terra", 272_000),
    ("gpt-5.6-luna", 272_000),
    ("gpt-daybreak-blue-latest", 272_000),
    ("codex-auto-review", 272_000),
    ("gpt-6.1", 272_000),
    ("gpt-6.1-terra", 500_000),
    ("gpt-daybreak-red-latest", 372_000),
];
/// Families: alias, display name and target, Claude's from the pinned SDK's catalog.
const FAMILY: &[(&str, &str, &str)] = &[
    ("fable", "Fable", "claude-fable-5-2"),
    ("opus", "Opus", "claude-opus-5-5"),
    ("sonnet", "Sonnet", "claude-sonnet-5-5"),
    ("haiku", "Haiku", "claude-haiku-5-5"),
    ("sol", "Sol", "gpt-6.1-sol"),
    ("terra", "Terra", "gpt-6.1-terra"),
    ("luna", "Luna", "gpt-6-luna"),
    ("astra", "Astra", "gpt-6-astra"),
    ("gemini", "Gemini", "gemini-3.8-flash"),
    ("deepseek", "DeepSeek", "deepseek-v4.1-flash-1017"),
    ("glm", "GLM", "glm-5.3-flash"),
];
/// The shipped providers: name, format, address and key variable; `[providers.<name>]` replaces
/// any of the last three.
const PROVIDERS: &[(&str, Transport, &str, &str)] = &[
    (
        "anthropic",
        Transport::Claude,
        "https://api.anthropic.com",
        "ANTHROPIC_API_KEY",
    ),
    (
        "google",
        Transport::Chat,
        "https://generativelanguage.googleapis.com/v1beta/openai",
        "GEMINI_API_KEY",
    ),
    (
        "openai",
        Transport::Responses,
        "https://api.openai.com/v1",
        "OPENAI_API_KEY",
    ),
    (
        "runinfra",
        Transport::Chat,
        "https://api.runinfra.ai/v1",
        "RUNINFRA_API_KEY",
    ),
];
const CHAT: &[(&str, Chat)] = &[
    (
        "gemini-3.8-flash",
        Chat {
            provider: "google",
            id: "gemini-3.8-flash",
            window: 1_048_576,
            top: Effort::High,
            replay: Replay::Streamed,
            variant: include_str!("data/variants/gemini-lean.json"),
        },
    ),
    (
        "deepseek-v4.1-flash-1017",
        Chat {
            provider: "runinfra",
            id: "deepseek-v4-1-flash-1017",
            window: 1_048_576,
            top: Effort::Max,
            replay: Replay::Streamed,
            variant: include_str!("data/variants/bash-coord.json"),
        },
    ),
    (
        "glm-5.3-flash",
        Chat {
            provider: "runinfra",
            id: "glm-5-3-flash",
            window: 1_048_576,
            top: Effort::Max,
            replay: Replay::Streamed,
            variant: include_str!("data/variants/bash-coord.json"),
        },
    ),
];

/// Natural family spellings become one stable alias; explicit wire IDs and unknown names
/// pass through unchanged. Every place a model choice enters Razor uses this normalizer.
pub(crate) fn id(name: String) -> String {
    if name.contains('/')
        || ((name.starts_with("claude-") || name.starts_with("gpt-"))
            && name.bytes().any(|b| b.is_ascii_digit())
            && !name
                .bytes()
                .any(|b| b.is_ascii_uppercase() || b.is_ascii_whitespace()))
    {
        return name;
    }
    let lower = name.to_ascii_lowercase();
    let words = || {
        lower
            .split(|c: char| c.is_whitespace() || c == '-')
            .filter(|w| !w.is_empty())
    };
    FAMILY
        .iter()
        .find(|(family, ..)| {
            words().filter(|w| w == family).count() == 1
                && words().all(|w| {
                    w == *family
                        || matches!(w, "claude" | "gpt")
                        || w.bytes().all(|b| b.is_ascii_digit() || b == b'.')
                })
        })
        .map_or(name, |(family, ..)| (*family).into())
}

/// A family's display name, such as `DeepSeek` for `deepseek`; `None` for any other name.
pub(crate) fn label(name: &str) -> Option<&'static str> {
    FAMILY.iter().find(|f| f.0 == name).map(|f| f.1)
}

fn resolved(name: &str) -> &str {
    FAMILY.iter().find(|f| f.0 == name).map_or(name, |f| f.2)
}

/// One authoritative model row: the normalized choice, else its resolved target.
pub(crate) fn settings<'a>(name: &str, config: &'a Config) -> Option<&'a crate::config::Model> {
    let name = id(name.into());
    config
        .models
        .get(&name)
        .or_else(|| config.models.get(resolved(&name)))
}

/// The chat endpoint `name` is reached at under `config`, when it takes video; `None` for
/// Claude, GPT and every other chat model.
pub(crate) fn video(name: &str, config: &Config) -> Option<Endpoint> {
    Some(model(name, config).ok()?.endpoint).filter(Endpoint::video)
}

/// Families first, then configured provider models, once each. Versioned shipped IDs stay
/// usable when typed, without appearing in pickers.
pub(crate) fn models(config: &Config) -> Vec<String> {
    let providers = config.providers.iter();
    let listed = providers.flat_map(|(p, t)| t.models.iter().map(move |id| format!("{p}/{id}")));
    let mut models = std::collections::BTreeSet::from_iter(listed);
    models.extend(config.models.keys().filter(|n| n.contains('/')).cloned());
    FAMILY
        .iter()
        .map(|(name, ..)| String::from(*name))
        .chain(models)
        .collect()
}

/// The route's choice, with known restrictions and descriptive costs; no account probe. A
/// provider's model takes the speeds its id takes on its own route.
pub(crate) fn speed_choice(name: &str, speed: Speed, config: &Config) -> SpeedChoice {
    use std::{collections::BTreeMap, sync::LazyLock};
    static TEXT: LazyLock<BTreeMap<String, String>> = LazyLock::new(|| {
        // The shipped speed catalogue is valid TOML matching the pricing table.
        toml::from_str(include_str!("data/speeds.toml")).expect("shipped speed catalogue")
    });
    let configured = settings(name, config).is_some_and(|r| r.speed_ids.contains_key(&speed));
    let route = route(name, config).map(|m| m.endpoint);
    let (transport, id) = route.map_or((Transport::Chat, name.into()), |e| (e.transport, e.id));
    let accepts = |key: &str| TEXT[key].split(',').any(|m| m == id);
    let key = match (speed, transport) {
        (Speed::Standard, _) => "standard",
        (Speed::Fast, Transport::Responses) => "gpt_fast",
        (Speed::Ultrafast, Transport::Responses) if accepts("ultrafast_models") => "gpt_ultrafast",
        (Speed::Fast, Transport::Claude) if accepts("claude_models") => "claude",
        (Speed::Fast, Transport::Chat) if id.starts_with("gemini-") => "gemini",
        (_, Transport::Chat) if configured => "configured",
        _ => "unavailable",
    };
    SpeedChoice {
        speed,
        reason: (key == "unavailable").then(|| TEXT[key].clone()),
        hint: TEXT[key].clone(),
    }
}
/// Rejects unavailable or conflicting speed choices before any settings are recorded.
pub(crate) fn speed(name: &str, speed: Speed, config: &Config) -> Result<(), Error> {
    if let Some(reason) = speed_choice(name, speed, config).reason {
        return Err(Error::Invalid(reason));
    }
    if name.starts_with("gemini-")
        && speed != Speed::Standard
        && settings(name, config)
            .and_then(|r| r.request.as_ref())
            .and_then(|r| r.get("service_tier"))
            .is_some_and(|v| v != "priority")
    {
        return Err(Error::Invalid(
            "request.service_tier conflicts with Fast; remove it or choose Standard.".into(),
        ));
    }
    Ok(())
}
