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
/// for Claude's and GPT's own sign-ins), its window when config or the catalog knows it, and on
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
    let row = config.models.get(name);
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
        _ if CLAUDE.contains(&name) || name.starts_with("claude-") => (None, name),
        _ if window(name).is_some() => {
            endpoint.transport = Transport::Responses;
            endpoint.url.clone_from(&config.responses.base_url);
            (None, name)
        }
        _ => {
            let names = CLAUDE.iter().chain(SHORT.iter().map(|(name, _)| name));
            let names = names.copied().collect::<Vec<_>>().join(", ");
            return Err(Error::Invalid(format!(
                "{name} is neither a model Razor ships nor a provider's model; check the name ({names}, or a full id from the model list), or name it <provider>/<id> with the provider under [providers]"
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

/// Resolves Razor's limits from live settings, else the catalog's or the SDK's window at most
/// `LARGE`'s, handed over at 0.8 or, at that size, `LARGE`'s share; a configured Claude window
/// never changes what its SDK sends.
pub(crate) fn limits(name: &str, config: &Config, reported: Option<u64>) -> (Option<u64>, f64) {
    let row = config.models.get(name);
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

/// GPT's shipped context windows, by id.
fn window(id: &str) -> Option<u64> {
    MODELS.iter().find(|(n, _)| *n == id).map(|(_, w)| *w)
}

const MODELS: &[(&str, u64)] = &[
    ("gpt-6-astra", 272_000),
    ("gpt-6.1-sol", 272_000),
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
const CLAUDE: &[&str] = &["opus", "sonnet", "haiku", "fable"];
/// The short GPT names, each for one id in `MODELS`.
const SHORT: &[(&str, &str)] = &[
    ("sol", "gpt-6.1-sol"),
    ("terra", "gpt-6.1-terra"),
    ("luna", "gpt-6-luna"),
    ("astra", "gpt-6-astra"),
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

/// The id a model name stands for: a short GPT name's full id, any other name unchanged. Each
/// place a name enters Razor calls it once, so records, status and requests hold full ids.
pub(crate) fn id(name: String) -> String {
    SHORT
        .iter()
        .find(|(short, _)| *short == name)
        .map_or(name, |(_, id)| (*id).to_owned())
}

/// The chat endpoint `name` is reached at under `config`, when it takes video; `None` for
/// Claude, GPT and every other chat model.
pub(crate) fn video(name: &str, config: &Config) -> Option<Endpoint> {
    Some(model(name, config).ok()?.endpoint).filter(Endpoint::video)
}

/// The model names Razor lists: the shipped ones, each `[models]` row's and each provider's
/// listed models as `<provider>/<id>`, sorted and once each.
pub(crate) fn models(config: &Config) -> Vec<String> {
    let shipped = MODELS.iter().map(|(name, _)| *name);
    let shipped = shipped
        .chain(CHAT.iter().map(|(name, _)| *name))
        .chain(CLAUDE.iter().copied());
    let providers = config.providers.iter();
    let listed = providers.flat_map(|(p, t)| t.models.iter().map(move |id| format!("{p}/{id}")));
    let mut models = std::collections::BTreeSet::from_iter(shipped.map(String::from).chain(listed));
    models.extend(config.models.keys().cloned());
    models.into_iter().collect()
}

/// The route's choice, with known restrictions and descriptive costs; no account probe. A
/// provider's model takes the speeds its id takes on its own route.
pub(crate) fn speed_choice(name: &str, speed: Speed, config: &Config) -> SpeedChoice {
    use std::{collections::BTreeMap, sync::LazyLock};
    static TEXT: LazyLock<BTreeMap<String, String>> = LazyLock::new(|| {
        // The shipped speed catalogue is valid TOML matching the pricing table.
        toml::from_str(include_str!("data/speeds.toml")).expect("shipped speed catalogue")
    });
    let configured = config
        .models
        .get(name)
        .is_some_and(|r| r.speed_ids.contains_key(&speed));
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
        && config
            .models
            .get(name)
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
