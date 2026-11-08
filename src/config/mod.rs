//! The live configuration; parsing is bounded and a bad edit never replaces the last good value.

pub mod cli;
pub(crate) mod manage;
mod mcp;
mod pack;
mod residents;
pub mod setup;
pub(crate) use pack::{Settings, roots};
pub(crate) use residents::{Day, Deliver, Resident, Schedule, Time};

use crate::{
    protocol::{Brand, Effort, Mcp, Phone, SessionId, Speed, Ui},
    tools,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fmt,
    path::{Path, PathBuf},
    sync::LazyLock,
    time::Duration,
};

/// The binary's canonical folder, resolved before the runtime starts; none if unavailable.
pub static INSTALL_DIR: LazyLock<Option<PathBuf>> = LazyLock::new(|| {
    std::env::current_exe()
        .and_then(std::fs::canonicalize)
        .ok()
        .map(|exe| exe.with_file_name(""))
});

/// The file `name` that Razor ships: beside the binary, else in the source tree it was built
/// from; None when neither has it. It looks on the calling thread, so the async loop calls it
/// through `spawn_blocking`.
pub(crate) fn shipped(name: &str) -> Option<PathBuf> {
    let beside = INSTALL_DIR.iter().map(|dir| dir.join(name));
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join(name);
    beside.chain([source]).find(|path| path.is_file())
}

/// Makes Razor's home and any missing parents owner-only (0700), since its records hold whole
/// conversations; a home that already exists keeps its mode. Serve, the clients that start it, and
/// setup call this before writing there, so whichever runs first makes the home.
pub(crate) fn make_home(home: &Path) -> Result<(), Error> {
    use std::os::unix::fs::DirBuilderExt;
    let made = std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(home);
    made.map_err(|source| Error::Other {
        source,
        context: "making Razor's home",
    })
}

/// One struct per TOML table; every field has a reader at the next turn or server start.
#[derive(Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub(crate) model: String,
    pub(crate) effort: Effort,
    /// The model and effort a session started for a call runs on, unless the caller names its
    /// own; a call to an existing session keeps that session's (docs/design/call.md).
    pub(crate) call_model: String,
    pub(crate) call_effort: Effort,
    pub(crate) speed: Speed,
    pub(crate) rebuild: Rebuild,
    pub(crate) sessions: Sessions,
    pub(crate) server: Server,
    /// `[remote]`: another machine's server this one's session commands use instead of its own.
    pub(crate) remote: Option<Remote>,
    guards: Guards,
    tools: BTreeMap<String, Script>,
    pub(crate) web: tools::web::Config,
    pub(crate) mcp: BTreeMap<String, Mcp>,
    /// `[[plugins]]`, in the order their handlers run (docs/design/extensions.md).
    pub plugins: Vec<Plugin>,
    pub(crate) children: Children,
    pub(crate) stuck: Stuck,
    profiles: Profiles,
    pub(crate) responses: Responses,
    /// `[providers.<name>]`: each provider's format, address, key variable and listed models, over
    /// a shipped provider of that name (docs/models.md#providers).
    pub(crate) providers: BTreeMap<String, Provider>,
    /// Razor itself reaches only the providers under `providers` (docs/security.md).
    pub(crate) provider_only: bool,
    /// `[models."<model>"]`: limits for every model, request settings for chat routes.
    pub(crate) models: BTreeMap<String, Model>,
    pub(crate) claude: Claude,
    /// `[residents.<name>]`: each resident, by name, which is its session's id.
    pub(crate) residents: BTreeMap<SessionId, Resident>,
    /// The program that carries an ask to the person (for example a notifier that waits for a
    /// reply). The question is its last argument; what it prints when it exits 0 is the answer.
    /// Empty: asks wait for a client.
    pub(crate) ask: Vec<String>,
    /// The program that carries a notification (for example a desktop or phone notifier). The
    /// text is its last argument.
    pub(crate) notify: Vec<String>,
    /// The effective pack's directory: the person's choice, or the starter (docs/design/packs.md).
    pack: Option<PathBuf>,
    /// More skill folders, searched after the pack's and the project's; a conversation's opening
    /// list omits their skills; `skill` still loads them by name.
    pub(crate) skills: Vec<PathBuf>,
    /// Update installed machine skills daily; project settings cannot change it.
    pub(crate) skill_updates: bool,
    /// Install newer releases daily; source installations never update automatically.
    pub(crate) release_updates: bool,
    /// The system prompt's entries in order: `razor`, `project`, files and folders, absolute once
    /// parsed (docs/design/profiles.md). Only the person's file and the pack may set it.
    startup: Vec<String>,
    pub(crate) brand: Brand,
    pub(crate) ui: Ui,
    pub(crate) phone: Phone,
    /// A program and its arguments that read a reply's text on standard input and write its
    /// MP3 audio to standard output, for `GET /v1/sessions/{id}/speech/{entry}`; empty, none.
    /// The speaker follows the arguments, a resident's name for its sessions, else the session's
    /// id, so a program can give an agent its own voice.
    pub(crate) speech: Vec<String>,
    /// A program and its arguments that read a voice note's audio on standard input and write its
    /// words to standard output, failing with a sentence as standard error's last line; empty,
    /// voice notes are refused.
    pub(crate) transcribe: Vec<String>,
    /// `[live]`: where live calls go and the names their voice will hear.
    pub(crate) live: LiveSettings,
    /// Local argv writing 16 kHz mono s16le PCM on stdout; empty disables recording.
    pub(crate) capture: Vec<String>,
    /// Local player argv: substitute {rate}, then append the audio path.
    pub(crate) play: Vec<String>,
    /// `[push]`: Apple's push service, a machine key; without it nothing is sent.
    pub(crate) push: Option<Push>,
    /// `[tasks]`: set only in a project's own `.razor/config.local.toml`.
    pub(crate) tasks: Tasks,
    pub(crate) memory: Memory,
}

/// One plugin file; handlers run in configuration order. Relative paths start at Razor's home.
#[derive(Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Plugin {
    pub(crate) file: PathBuf,
    /// Command hooks handle these events; JavaScript files register their own.
    #[serde(default)]
    pub(crate) before: Vec<Event>,
    /// Command hooks observe entries of these kinds.
    #[serde(default)]
    pub(crate) after: Vec<String>,
}

/// The only events whose handlers may change or refuse pending work.
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Event {
    Prompt,
    Input,
    Call,
    Rebuild,
}

/// `[tools.<name>]`: a script plus a schema. Each call runs `command` with bash, as a job in the
/// session's `cwd`, with the call's input as JSON on its standard input; the script checks that
/// input itself, as an MCP server does. `input`'s root must be `{"type": "object"}`, or tools
/// leaves the tool out.
#[derive(Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Script {
    pub(crate) command: String,
    pub(crate) description: String,
    pub(crate) input: serde_json::Value,
    #[serde(default)]
    pub(crate) read_only: bool,
}

/// How a project's tasks are kept.
#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Tasks {
    /// A remote of the project's checkout; its tasks are shared through `refs/razor/tasks` there.
    /// Absent, they are plain files in Razor's home.
    pub(crate) remote: Option<String>,
}

/// The nightly pass and optional project memory sharing.
#[derive(Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Memory {
    pub(crate) night: DreamNight,
    pub(crate) model: Option<String>,
    pub(crate) remote: Option<String>,
}
impl Default for Memory {
    fn default() -> Self {
        Self {
            night: DreamNight::At(Time(3 * 3600)),
            model: None,
            remote: None,
        }
    }
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(untagged, expecting = "a time of day or false")]
pub(crate) enum DreamNight {
    At(Time),
    Off(#[serde(deserialize_with = "disabled")] bool),
}
/// Only false disables dreaming; true is not a time.
fn disabled<'de, D: serde::Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    match bool::deserialize(d)? {
        false => Ok(false),
        true => Err(serde::de::Error::custom("use a time or false")),
    }
}
/// A machine key: calls go to OpenAI, or to `url`, which speaks GPT-Live's protocol (a stand-in, or
/// the person's own relay); either gets `OPENAI_API_KEY` when it is set.
#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct LiveSettings {
    pub(crate) url: Option<String>,
    pub(crate) names: Vec<String>,
}
/// Unset, the threshold is the shipped one for the model's window (`profiles::limits`).
#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Rebuild {
    pub(crate) threshold: Option<f64>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Sessions {
    pub(crate) idle: usize,
    pub(crate) handover_minutes: u64,
    pub(crate) handover_tokens: u64,
    /// Zero leaves conversations open until the person closes them.
    pub(crate) close_minutes: u64,
}
#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Server {
    pub(crate) address: Option<String>,
    pub(crate) https_url: Option<String>,
    /// The machine's shorthand the other servers pair this one under (`desk`, `laptop`): the
    /// first word of every root's name, read at each root's start. Empty: its project's.
    pub(crate) name: String,
    /// How a phone names this home. Empty: `name`, else the machine's name.
    pub(crate) label: String,
    pub(crate) realm: String,
    /// Stable server.id keeping shared settings; empty means this home, never a peer name.
    pub(crate) shared: String,
    /// A command `razor install`'s service runs the server through, the server's own command
    /// as its last arguments; read only by `razor install`.
    pub(crate) wrapper: Vec<String>,
}
/// The other server's `server.address`, the file holding the key it enrolled this machine with,
/// and the folder there that new sessions start in, else the launch folder's path.
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Remote {
    pub(crate) address: String,
    pub(crate) key: PathBuf,
    pub(crate) folder: Option<PathBuf>,
}
/// The Apple developer team and the app's bundle id, and a token signing key for each
/// environment this server sends to; nothing goes to a device whose environment has none.
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Push {
    pub(crate) team: String,
    pub(crate) bundle: String,
    pub(crate) development: Option<PushKey>,
    pub(crate) production: Option<PushKey>,
    /// Where pushes go instead of Apple's host: the flows' stand-in.
    pub(crate) endpoint: Option<String>,
}
/// A key's id and the path of its `.p8` file outside any repository; its bytes go nowhere but
/// the signature.
#[derive(Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PushKey {
    pub(crate) key_id: String,
    pub(crate) key: PathBuf,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
struct Guards {
    delete: bool,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Children {
    /// Startup instructions for every child, independent of its parent's list.
    pub(crate) startup: Vec<String>,
    pub(crate) max: usize,
    #[serde(deserialize_with = "duration", serialize_with = "duration_text")]
    pub(crate) grace: Duration,
    /// Files whose text replaces Razor's reviewer or researcher text, read at each start.
    pub(crate) reviewer: Option<PathBuf>,
    pub(crate) researcher: Option<PathBuf>,
    /// The model a video's watcher runs on; one that cannot see video starts none
    /// (docs/design/video.md).
    pub(crate) watcher: String,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Stuck {
    #[serde(deserialize_with = "duration", serialize_with = "duration_text")]
    pub(crate) after: Duration,
}

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
struct Profile {
    pub(crate) code_mode: Option<bool>,
    /// Codex's image generation, which only GPT on the sign-in has.
    image_gen: Option<bool>,
}
#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
struct Profiles {
    claude: Profile,
    codex: Profile,
}
/// One model's settings, each replacing its shipped row's (docs/design/generic.md); a machine
/// key, refused in a project's file.
#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Model {
    pub(crate) speed_ids: BTreeMap<Speed, String>,
    /// Context tokens; when set, the session asks for a handover at its rebuild threshold.
    pub(crate) window: Option<u64>,
    pub(crate) threshold: Option<f64>,
    pub(crate) replay: Option<Replay>,
    /// Top-level fields added to every request, such as a host's switch for keeping history.
    pub(crate) request: Option<serde_json::Map<String, serde_json::Value>>,
    /// The variant file's declarations, read with the settings file that names it.
    pub(crate) variant: Option<serde_json::Value>,
}
/// How a chat message's reasoning goes back: as it streamed, moved to one of the two fields
/// hosts read, or not at all.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Replay {
    #[default]
    Streamed,
    Reasoning,
    ReasoningContent,
    None,
}

/// A provider: its format (`chat` when neither config nor a shipped provider names one), its
/// address, the variable holding its key (none: no auth; it never holds a key), and the model ids
/// Razor lists for it. A field left out is the shipped provider's.
#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Provider {
    pub(crate) format: Option<crate::profiles::Transport>,
    pub(crate) base_url: Option<String>,
    pub(crate) key: Option<String>,
    pub(crate) models: Vec<String>,
}

/// The ChatGPT sign-in's address.
#[derive(Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Responses {
    pub(crate) base_url: String,
}
/// `[claude]`, a machine key: a project's file cannot set it.
#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Claude {
    /// A command printing a Claude sign-in token, run for each conversation Razor opens on
    /// Claude; empty: the server's own sign-in (docs/design/claude.md).
    pub(crate) sign_in: Vec<String>,
    /// A JSON file another program keeps with each account's plan usage, read for the model
    /// rows of the account `sign_in` names (docs/design/claude.md).
    pub(crate) usage_file: Option<PathBuf>,
}
impl Default for Responses {
    fn default() -> Self {
        Self {
            base_url: "https://chatgpt.com/backend-api/codex".into(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            model: "opus".into(),
            effort: Effort::High,
            call_model: "terra".into(),
            call_effort: Effort::Low,
            speed: Speed::Standard,
            rebuild: Rebuild::default(),
            sessions: Sessions::default(),
            server: Server::default(),
            remote: None,
            guards: Guards::default(),
            tools: BTreeMap::new(),
            web: Default::default(),
            mcp: BTreeMap::new(),
            plugins: Vec::new(),
            children: Children::default(),
            stuck: Stuck::default(),
            profiles: Profiles::default(),
            responses: Responses::default(),
            providers: BTreeMap::new(),
            provider_only: false,
            models: BTreeMap::new(),
            claude: Claude::default(),
            residents: BTreeMap::new(),
            ask: Vec::new(),
            notify: Vec::new(),
            pack: None,
            skills: Vec::new(),
            skill_updates: true,
            release_updates: true,
            startup: vec!["razor".into(), "project".into()],
            brand: Default::default(),
            ui: Default::default(),
            phone: Default::default(),
            speech: Vec::new(),
            transcribe: Vec::new(),
            live: LiveSettings::default(),
            capture: Vec::new(),
            // Only macOS ships a player, `afplay`; elsewhere none is assumed.
            #[cfg(target_os = "macos")]
            play: vec!["afplay".into(), "-r".into(), "{rate}".into()],
            #[cfg(not(target_os = "macos"))]
            play: Vec::new(),
            push: None,
            tasks: Tasks::default(),
            memory: Memory::default(),
        }
    }
}
impl Default for Sessions {
    fn default() -> Self {
        Self {
            idle: 4,
            handover_minutes: 50,
            handover_tokens: 50_000,
            close_minutes: 0,
        }
    }
}
impl Default for Guards {
    fn default() -> Self {
        Self { delete: true }
    }
}
impl Default for Children {
    fn default() -> Self {
        Self {
            startup: vec!["project".into()],
            max: 20,
            grace: Duration::from_secs(180),
            reviewer: None,
            researcher: None,
            watcher: "gemini-3.8-flash".into(),
        }
    }
}
impl Default for Stuck {
    fn default() -> Self {
        Self {
            after: Duration::from_secs(900),
        }
    }
}

fn duration<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Duration, D::Error> {
    let text = String::deserialize(d)?;
    let text = text.trim();
    let unit = text.trim_start_matches(|c: char| c.is_ascii_digit());
    let n = &text[..text.len() - unit.len()];
    let n: u64 = n.parse().map_err(serde::de::Error::custom)?;
    let factor = match unit {
        "s" => 1,
        "m" => 60,
        "h" => 3600,
        "d" => 86400,
        _ => {
            return Err(serde::de::Error::custom(
                "a duration is a number followed by s, m, h or d",
            ));
        }
    };
    n.checked_mul(factor)
        .map(Duration::from_secs)
        .ok_or_else(|| serde::de::Error::custom("the duration is too large"))
}

fn duration_text<S: serde::Serializer>(value: &Duration, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&format!("{}s", value.as_secs()))
}

impl Config {
    /// Loads machine settings once; live readers retain a Settings handle for per-file fallback.
    pub async fn load(home: &Path) -> Result<Self, Error> {
        Ok(Settings::open(home.to_path_buf())?.config)
    }

    fn parse(table: toml::Table) -> Result<Self, Error> {
        // A row written before providers is told where its fields moved; nothing migrates it.
        let rows = table.get("models").and_then(toml::Value::as_table);
        let old = |(_, row): &(_, &toml::Value)| {
            ["base_url", "key", "id"].map(|f| row.get(f)) != [None; 3]
        };
        if let Some((name, _)) = rows.into_iter().flatten().find(old) {
            let moved = "cannot hold base_url, key or id: they moved to [providers.<name>] as base_url, key and models, and the model is named <provider>/<id> (docs/configuration.md#providers).";
            return Err(Error::Invalid(format!("[models.{name}] {moved}")));
        }
        let mut config: Self = table.try_into().map_err(|e: toml::de::Error| {
            let why = e.message().trim_end().replace('\n', " ");
            Error::Invalid(format!("The config is invalid: {why}."))
        })?;
        config.model = crate::profiles::id(std::mem::take(&mut config.model));
        if (config.rebuild.threshold).is_some_and(|t| !t.is_finite() || !(0.0 < t && t <= 1.0)) {
            return Err(Error::Invalid(
                "rebuild.threshold must be greater than zero and at most one.".into(),
            ));
        }
        for list in [&config.web.search, &config.web.fetch, &config.web.answer] {
            if list.iter().enumerate().any(|(i, p)| list[..i].contains(p)) {
                return Err(Error::Invalid(
                    "Each [web] list must name each provider at most once; remove duplicates."
                        .into(),
                ));
            }
        }
        let count = config.models.len();
        config.models = std::mem::take(&mut config.models)
            .into_iter()
            .map(|(name, row)| (crate::profiles::id(name), row))
            .collect();
        if config.models.len() != count {
            return Err(Error::Invalid(
                "Two model rows name the same model; keep one row.".into(),
            ));
        }
        if config.web.answer.contains(&tools::web::Provider::Firecrawl) {
            return Err(Error::Invalid(
                "Firecrawl has no answer action; use exa or parallel in web.answer.".into(),
            ));
        }
        Ok(config)
    }

    /// What the merged settings name across files: each provider's name and address, each model
    /// row against its route, and with `provider_only` a default model on a provider.
    fn check(self) -> Result<Self, Error> {
        let invalid = |e: crate::profiles::Error| Error::Invalid(e.to_string());
        for (name, provider) in &self.providers {
            let url = provider.base_url.as_deref().map(reqwest::Url::parse);
            if url.is_some_and(|u| !u.is_ok_and(|u| matches!(u.scheme(), "http" | "https"))) {
                let why = format!("[providers.{name}] base_url must be an HTTP or HTTPS URL.");
                return Err(Error::Invalid(why));
            }
        }
        for (name, row) in &self.models {
            crate::profiles::validate(name, row, &self).map_err(invalid)?;
        }
        if self.provider_only {
            crate::profiles::model(&self.model, &self).map_err(invalid)?;
        }
        Ok(self)
    }

    /// The selected family and code-mode setting bind one snapshot of available actions, as a
    /// child's when `child` is set. With `provider_only`, Claude Code's WebFetch, which fetches
    /// from this machine, is left out.
    pub(crate) fn profile(
        &self,
        home: &Path,
        model: &str,
        actions: std::sync::Arc<[tools::Action]>,
        role: crate::profiles::Builtins,
    ) -> Result<crate::profiles::Profile, crate::profiles::Error> {
        use crate::profiles::{self, Transport};
        let route = profiles::model(model, self)?;
        let mode = role != profiles::Builtins::Dream
            && match route.endpoint.transport {
                Transport::Claude => self.profiles.claude.code_mode.unwrap_or(false),
                Transport::Responses => {
                    (self.profiles.codex.code_mode).unwrap_or(route.endpoint.key.is_none())
                }
                Transport::Chat => false,
            };
        let variant = self.models.get(model).and_then(|c| c.variant.as_ref());
        // Codex's image generation runs on the ChatGPT sign-in: GPT there gets it, and Claude and
        // chat a helper on it, never a provider's GPT.
        let image = (route.endpoint.key.is_none()
            || route.endpoint.transport != Transport::Responses)
            && !self.provider_only
            && home.join("auth.json").exists()
            && self.profiles.codex.image_gen.unwrap_or(true);
        let actions = if role == profiles::Builtins::Dream {
            actions
                .iter()
                .filter(|a| profiles::dream_tool(&a.name))
                .cloned()
                .collect::<Vec<_>>()
                .into()
        } else {
            actions
        };
        let mut profile = profiles::load(
            route,
            mode,
            variant,
            actions,
            role,
            image && role != profiles::Builtins::Dream,
        )?;
        if self.provider_only {
            profile.tools.retain(|tool| tool["builtin"] != "WebFetch");
        }
        Ok(profile)
    }

    /// The startup list `session` opens with: its own when it is a resident that names one.
    pub(crate) fn startup_for(&self, session: &crate::protocol::SessionId) -> &[String] {
        let resident = self.residents.get(session);
        resident
            .and_then(|r| r.startup.as_deref())
            .unwrap_or(&self.startup)
    }
}

/// The home skill folders imported from the other agent tools, in lookup order.
pub(crate) fn home_skills() -> Vec<PathBuf> {
    std::env::var_os("HOME")
        .into_iter()
        .flat_map(|home| {
            [".agents", ".claude", ".codex"].map(|dir| Path::new(&home).join(dir).join("skills"))
        })
        .collect()
}

impl Settings {
    /// Skill lookup: machine overrides, pack, project, home, then configured libraries.
    pub(crate) fn skills(&self) -> Vec<PathBuf> {
        let pack = self.pack_skills();
        let project = self
            .project
            .iter()
            .flat_map(|root| [".agents", ".claude"].map(|dir| root.join(dir).join("skills")));
        self.config
            .skill_updates
            .then(|| self.skill_overrides())
            .into_iter()
            .chain(pack)
            .chain(project)
            .chain(home_skills())
            .chain(self.config.skills.iter().cloned())
            .collect()
    }

    /// Freezes session actions and their local MCP root from this settings snapshot; the caller
    /// adds the plugins' tools. With `provider_only` there are no web verbs, and the MCP servers a
    /// project's shared `.razor/config.toml` brings stay off (docs/security.md).
    pub(crate) fn tools(&self) -> tools::Config {
        use crate::protocol::McpScope as Mcp;
        let skills = self.skills();
        let only = self.config.provider_only;
        let shared = self.destination(Mcp::Project).ok().filter(|d| only && d.1);
        let mut mcp = self.config.mcp.clone();
        for (name, server) in &mut mcp {
            let source = self.sources.get(&format!("mcp.{name}"));
            server.enabled &= shared.as_ref().is_none_or(|d| source != Some(&d.0));
        }
        tools::Config {
            delete_guard: self.config.guards.delete,
            described: skills.len() - self.config.skills.len(),
            skills,
            scripts: self.config.tools.clone(),
            mcp,
            mcp_cwd: self.project.clone(),
            web: if only {
                Default::default()
            } else {
                self.config.web.clone()
            },
            plugins: BTreeMap::new(),
        }
    }
}

/// Invalid input is actionable; machine failures keep their source and context.
#[derive(Debug)]
pub enum Error {
    Invalid(String),
    Other {
        source: std::io::Error,
        context: &'static str,
    },
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(text) => f.write_str(text),
            Self::Other { source, context } => write!(f, "Razor failed {context}: {source}."),
        }
    }
}
impl std::error::Error for Error {}
