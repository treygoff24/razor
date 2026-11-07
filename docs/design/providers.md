# Providers, keys and contacting only them

Trey, 6 November 2026: setup offers "Claude sub, ChatGPT sub, both, or provider API key"; "we need a clean, simple solution for allowing users to configure almost any API key backed provider", supported in setup and doctor; and a separate setup question that keeps Razor to its model provider on a work machine, under which MCP servers the person adds stay allowed. The research behind it read Pi and OpenCode: both keep one owner-only key store, describe a provider as an API, an address, a key and its models, and keep background calls on the selected provider. Neither tests the key in its health check or has one switch that keeps every outside service off, and both make up context windows for unknown models. Razor does neither.

## Public surface

```rust
// src/profiles/models.rs
/// The wire a model is reached over, which is also a provider's `format`: "anthropic", "responses", "chat".
pub enum Transport { Claude, Responses, Chat }
/// A model's route under the live settings: where its requests go, the provider it is on (none for
/// Claude's and GPT's own sign-ins), its window when config or the catalog knows it, and on chat the
/// variant it gets when config names none.
pub struct Model { pub endpoint: Endpoint, pub provider: Option<String>, window: Option<u64>, variant: &'static str }
/// Claude's names on the SDK with its own sign-in, GPT's on Razor's ChatGPT sign-in at `[responses]
/// base_url`, a shipped chat name on its provider, `<provider>/<id>` on that provider; with
/// `provider_only`, only a provider under `[providers]`, else a sentence naming the setting.
pub fn model(name: &str, config: &Config) -> Result<Model, Error>;
/// The names `/model` lists: shipped ones, `[models]` rows and each provider's `models`.
pub fn models(config: &Config) -> Vec<String>;

// src/profiles/mod.rs
/// Where a profile's requests go: the transport, the sign-in's base or a provider's address, its key
/// variable (`None` on a sign-in, `Some("")` for no auth), the id, the top effort and the request fields.
pub struct Endpoint { pub transport, pub url, pub key: Option<String>, pub id, pub top, pub speed_ids, pub replay, pub request }

// src/config/mod.rs
/// `[providers.<name>]`, over a shipped provider of that name, field by field.
pub struct Provider { pub format: Option<Transport>, pub base_url: Option<String>, pub key: Option<String>, pub models: Vec<String> }
/// `provider_only`: Razor itself reaches only the providers under `providers`.
pub provider_only: bool,

// src/config/setup.rs
/// Sets each variable keys.toml lists that the environment lacks; `environment` keeps those names, with the
/// web and voice keys, out of agents' shells. `source` says keys.toml or the environment, never a value.
pub fn load(home: &Path) -> Result<(), Error>;
pub(crate) fn environment() -> BTreeMap<String, String>;
pub(crate) fn source(home: &Path, name: &str) -> Option<&'static str>;

// src/api/ready.rs
/// One tool-free answer on a model's own route (Claude Code with a provider's variables, or Razor's
/// loop), its failure one sentence naming what to check: recaps and doctor share it.
pub(super) async fn answer(home, responses, cwd, route: &Model, session, prompt, text) -> Result<String, String>;
/// Doctor's line for the default model and whether it can run; on a provider, after one test request.
pub async fn default(home, config, checks) -> (String, bool);

// src/claude/mod.rs
/// A provider conversation's variables in place of every sign-in: ANTHROPIC_BASE_URL, the key as both
/// ANTHROPIC_API_KEY and ANTHROPIC_AUTH_TOKEN, CLAUDE_CONFIG_DIR in the home, no nonessential traffic.
pub(crate) fn provider(endpoint: &Endpoint, folder: &Path) -> Result<Value, Error>;
```

## Data flow

1. Main loads keys.toml before any thread: each listed variable the environment lacks is set, and the file's names join the withheld list the controller strips from agents' shells and MCP servers.
2. Every place a model name is used asks `profiles::model(name, config)`: the controller through `Config::profile` at each rendering, so a session on a refused model fails when its conversation opens; readiness, the model list, recaps, speeds, the video watcher and doctor. The controller then reads the route from its profile's endpoint, not from the name.
3. Chat and Responses endpoints go to Razor's own loop. A key variable's value is the bearer, read for each request; `None` is Razor's ChatGPT sign-in. Code mode, image generation and `web.run` exist only on the sign-in.
4. An Anthropic-format endpoint runs through the shared Claude bridge, with `claude::provider`'s variables as the conversation's environment and the provider's id as the model. A provider's version names its address, so a switch never keeps a native conversation across hosts.
5. Recaps run on the session's own route: Haiku on Claude's sign-in, Terra on the ChatGPT sign-in (Trey, 4 November), a provider's session on its own model. Doctor sends the default model, on a provider, the same one-shot answer: "Reply with OK."
6. `provider_only` gates the reach Razor makes by itself: `profiles::model` (sessions, children, residents, recaps, the watcher), `Devices::paired` and `peer` (every paired-server contact), discovery and `devices add --address`, release and skill updates (not `razor update`, which the person types), the web verbs, Claude Code's WebFetch, the speech and transcription programs, live calls, the watcher, and the MCP servers of a project's shared file. Setup skips the browser download and the connect question under it.

## The hard decision: what a provider is, and how a session names its models

Three shapes were drawn. Additive: keep `[models."<name>"] base_url, key, id` and add a provider table beside it, which gives two ways to say where a model lives. Consolidating: a provider owns address, API and key; a model row keeps only what is the model's (window, threshold, request, replay, variant, speed ids); a session names `<provider>/<id>`, as Pi and OpenCode do. Subtractive on top of that: the built-in API-key switches become providers too, so `[responses] base_url = "https://api.openai.com/v1"` and the `~/.razor/key` file go, and GPT on a key is `openai/gpt-6.1-sol` (ruled 6 November 2026); the host match behind it, the key file branch of the sign-in and doctor's separate OpenAI line go with it.

The consolidating and subtractive shape was chosen. One fact has one place: a gateway serving many models states its address and key once, and a model's window lives in the one row every model already had. The cost is that the route depends on settings, so `profiles::model` takes the config, and the controller reads the route from the rendered profile. The shipped Gemini, DeepSeek and GLM names stay as they were, now on shipped `google` and `runinfra` providers that a table of the same name overrides. Claude on a plain key stays Claude Code's own behaviour (ANTHROPIC_API_KEY bills every native Claude session); `anthropic/opus` is the explicit route, and the only Claude route `provider_only` allows, since sign-ins never count under it (ruled 6 November 2026).

No context window is invented: a provider's model has one only from its `[models]` row, the GPT catalog by id, or Claude Code's report, and doctor says when it has none.
