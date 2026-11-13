# First run

`razor setup` prepares one Razor home on Linux or macOS in the person's terminal: choose Claude, ChatGPT, both, or an API key, then on a key whether Razor contacts nothing but the model provider, whether the server starts at login, then use the provider's own sign-in. It installs the browser, runs doctor and offers the terminal app. Trey approved the shorter first run on 4 November 2026: "I totally agree w/ ur streamlining of the onboarding, we should fs do that." On the same day he approved command-led help and recovery ("Full send"); the acceptance check is a fresh GPT agent using only the docs and CLI to learn and use Razor.

```text
/// List every CLI command, subcommand and option without reading the home or contacting a server.
razor --help | razor -h | razor help
/// Check prerequisites without starting a server; every problem names a fix command.
/// Claude Code may create its own configuration directory while checking sign-in.
/// Exit 0 when settings load and the default model can run, which on a provider takes one test
/// request; optional missing keys do not fail it.
razor doctor
/// Offer the background service, sign in where needed, check the machine and offer a first message.
razor setup [--console]
/// Save optional web and voice keys with hidden entry; Enter keeps each saved value.
razor keys
/// Stop this home's server, without changing its login-start setting; succeed when it is already stopped.
razor stop
/// Stop this home's server and start it again through its installed service, or detached otherwise.
razor restart
```

```rust
// src/config/setup.rs; config::Error
/// The optional keys `razor keys` asks for: the web providers' and ElevenLabs' for voice.
pub fn names() -> impl Iterator<Item = &'static str>;
/// Before threads start, fill absent environment variables from keys.toml, any variable's name;
/// exported values win. Invalid keys produce a sentence containing no value.
pub fn load(home: &Path) -> Result<(), Error>;
/// This process's environment less `names`, every name keys.toml listed and Claude's keys: what
/// agents' shells, MCP servers and Cua Driver's daemon start from.
pub(crate) fn environment() -> BTreeMap<String, String>;
/// keys.toml or the environment, never the value; None when unset.
pub(crate) fn source(home: &Path, name: &str) -> Option<&'static str>;
/// Require a terminal and save all completed key answers together at 0600; cancellation saves none.
/// A changed file tells the person to run razor restart if a server is already running.
pub fn keys(home: &Path) -> Result<(), Error>;
/// True only when doctor passed and the person accepted the terminal app; requires terminal I/O.
/// Console selects Anthropic Console billing; the default uses a Claude subscription.
pub async fn run(home: &Path, console: bool) -> Result<bool, Error>;

/// A shipped tool executable, or its PATH name for source builds.
pub fn tool(name: &str) -> PathBuf;

// src/residents/install.rs; residents::Error; re-exported by residents/mod.rs
/// Take an exclusive flock on this home's server.lock and write the holder's PID.
/// Serve retains the open descriptor until the process exits, including runtime teardown.
pub fn server_lock(home: &Path) -> Result<File, Error>;
/// Runtime shutdown waits at most TEARDOWN_TIMEOUT (1s), then process exit releases the lock.
/// SIGTERM this home's lock holder and await release within the API + runtime bounds plus 1s.
/// An unused or absent lock succeeds; signal and timeout errors fail without SIGKILL.
pub async fn stop(home: &Path) -> Result<(), Error>;
/// Start this home's installed service; false if absent or owned by another home.
/// Unreadable files and manager failures fail; only install refuses another home's service.
pub async fn start(home: &Path) -> Result<bool, Error>;
/// Read this home's installed service file and label without contacting its manager.
/// Another home's file or an absent file returns None; an unreadable file fails.
pub async fn installed(home: &Path) -> Result<Option<(PathBuf, String)>, Error>;
/// Whether this home has a service; a file belonging to another home gives install's refusal.
pub async fn check_service(home: &Path) -> Result<bool, Error>;

// src/api/mcp.rs; api::Error
/// Validate shell words without contacting the server and produce its management request.
pub fn parse(home: &Path, words: &[String]) -> Result<(&'static str, Value), Error>;
/// Send the validated request and render the response for the person.
pub async fn run(client: &reqwest::Client, key: &str, request: (&str, Value)) -> Result<String, Error>;

```

1. Help prints before reading state and includes `--version`, every launch option, every top-level command and the `devices`/`mcp` subcommands and options. Each command accepts `--help`/`-h` before state access, and `help <command>` prints the same syntax. The full help describes output modes, exit codes, `RAZOR_HOME` and the installed docs. Device and MCP argument validation happens before connecting; malformed commands exit 2 without starting a server. Usage failures end with `Run razor --help.` at the CLI's error boundary. Before threads start, `serve` and `doctor` load keys; exported values survive setup and its child processes. Each new server and doctor reads keys from disk.
2. Setup requires terminal stdin and stdout, loads config before asking, chooses Claude/ChatGPT/both/key (Enter is Claude), and creates the owner-only home. ChatGPT alone writes `model = "sol"` only if the person's config has no model; existing settings survive a rerun. A key asks whose: `anthropic`, `openai`, `google`, `runinfra`, or another endpoint (Enter), whose API (chat, responses or anthropic; Enter is chat), address, model id and context window (Enter: unknown) it asks in turn, writing `[providers.<host's first label>]` with `format`, `base_url`, `key = "<NAME>_API_KEY"` and `models`, and the model's `window` row when given. The key is typed hidden (the `keys` reader) and saved in keys.toml at 0600; Enter keeps a saved one, and for another endpoint with none saved removes `key`, a host without auth. The provider's model becomes `model`, and `call_model` when the settings name none (`anthropic/opus`, `openai/gpt-6.1-sol`, `gemini-3.8-flash`, `deepseek-v4.1-flash-1017`, or `<name>/<id>`). Then "Contact nothing but your model provider? (for work machines)" (default no): yes writes `provider_only = true` and an empty `[providers.<name>]` table for a shipped provider, which that setting needs (docs/design/providers.md). The server's name is written where the settings have none; pairing needs nothing more (docs/design/iroh.md). One compare-and-replace writes the person's file.
3. The background question stays before sign-in: yes runs `razor install`, which stops this home's lock holder before enabling the service. This preserves today's order and avoids starting a detached server through ChatGPT login before service installation.
4. Credential checks reuse doctor's checks. Plain setup skips already signed-in providers and respects configured Claude sign-in commands. Explicit `--console` always runs Claude sign-in when Claude is selected, allowing an account switch. Otherwise Claude uses `razor login claude` (or `razor login console` for Console billing) and ChatGPT uses `razor login`; both use the server's sign-in stream. There is no separate billing question in setup. A failed sign-in retains its cause and names the next command (`razor login claude` for Claude, `razor login` for ChatGPT, the matching `razor mcp sign-in <name>` for MCP).
5. Setup runs the shipped `agent-browser install` (PATH's for source builds), except with `provider_only`, where it says how to run it; Linux failures name `--with-deps`, which the person runs themselves. Then `razor doctor` must succeed before the offer to open the terminal app; main opens it in the same process.
6. `keys` reuses the current bounded hidden-entry reader and compare-and-replace write of `$RAZOR_HOME/keys.toml` at 0600. It collects Exa, Firecrawl, Parallel and ElevenLabs answers before writing; Enter retains a saved key. Ctrl-C/Ctrl-D during key entry restores the terminal and exits unsuccessfully without saving the collected changes. Setup's questions use the terminal's own line editing; interruption exits and setup can be rerun.
7. Doctor's `Model:` line names the default model's route: Claude's or Razor's ChatGPT sign-in with its check, or a provider, its format and address, the model id, its key's variable and whether it came from keys.toml or the environment (never the value), and its context window or that none is known. On a provider it sends one test request, "Reply with OK.", through the same one-shot answer recaps use (Claude Code with the provider's variables for an Anthropic-format one, Razor's own loop otherwise), within 60 seconds; a failure is one sentence naming what to check: the key, `base_url` and `format` on a 404, the model id on another refusal, the network and `base_url` when it cannot connect, `format` when the reply is not that API's. Doctor exits 0 only when the default model can run. Every doctor problem line ends with a concrete command that addresses its cause: the appropriate sign-in command, `razor keys`, or an exact settings-edit, prerequisite-install or permissions command. It retains the cause, exposes no credential and starts no server. Claude Code may create its configuration directory during its own sign-in check. Missing optional web/ElevenLabs keys name `razor keys` and do not fail doctor. Doctor also probes the browser and desktop executables with `--version` (Chrome launch and desktop operation remain untested), checks an authenticated server answer within two seconds, and reads service-file ownership; a stopped server explains that the next command starts it, and an absent service names `razor install`; a service file belonging to another home prints install's ownership refusal. On macOS, with the service's server running and a privacy permission missing, doctor in a terminal asks "Let agents see the screen and control this Mac? [y/N]"; yes has that server, and only it, ask macOS for Accessibility and Screen Recording, which puts Razor in both lists in System Settings. It prints no unconditional memory claim. MCP list renders names and states, an empty list says how to add a server, and import labels its preview and names `/mcp import` for applying it. The starter pack's existing settings skill is the one instruction for agents to point the person to `razor keys`, then `razor restart`, when a requested capability needs a missing key; the existing speech error also changes to `razor keys`.
8. `start` shares install's service path, label and stored-home ownership check. Linux uses `systemctl --user start`; macOS uses `launchctl kickstart` for a loaded agent or `bootstrap` for an unloaded one. A file owned by another home counts as no service for start, allowing a detached server for this home; install refuses the name collision and doctor prints that same refusal. If install cannot enable the service, it removes the file only when this install created it, so a missing manager cannot block the next connect after a first install. Reinstall retains an existing owned unit when the manager cannot answer and says that it kept it. Manager failures for an installed service remain failures; connect never spawns a detached competitor against that service.
9. `serve` takes an exclusive `flock` before initialization, writes its PID to the locked file, and keeps the descriptor open until process exit, including runtime teardown. Closing another descriptor for the same file cannot release this lock. Stop first tries a nonblocking shared lock: an absent or unused lock succeeds, and simultaneous stop probes cannot make each other read a stale PID. Serve alone takes an exclusive lock. Only while the lock remains held does stop read a bounded PID and send SIGTERM, then wait for release. A held lock with no valid PID is a server between taking the lock and writing its PID; stop refuses and says to run it again. Permission failures fail with their cause, and a timeout names `razor stop` again. `server.json` and the socket are never authority to signal a process. The wait is the API shutdown constant (20s) plus the runtime teardown constant (1s), plus one second for scheduling, without SIGKILL or a replacement on timeout. Serve uses `Runtime::shutdown_timeout(TEARDOWN_TIMEOUT)` after its main future returns, so a blocked runtime task cannot hold the lock indefinitely through teardown.
10. Restart is stop then `connect`: start the installed service, otherwise spawn detached, then await an authenticated answer within the existing ten-second readiness bound. If the detached child exits and no server holds the lock, connect spawns one replacement within that same deadline; a second such exit fails with the log's last line. Install also stops the lock holder before enabling the service. These commands accept no arguments and affect only the selected `RAZOR_HOME`. Stop does not unload the service: SIGTERM exits successfully and neither service restarts a clean exit.

**Decision: keep flock exclusion and store the holder's PID under it.** POSIX fcntl locks are process-owned: closing any descriptor for the file releases the lock. GPT's read/write/edit tools run in the server process, so reading `server.lock` made stop report success while the server kept running and allowed a second server to start. Flock belongs to the open file description and survives those unrelated closes. The holder writes its PID after acquisition, and stop trusts it only while a shared try fails. Shared probes do not exclude one another: an exclusive probe briefly held by another stop looked like a live server and allowed its stale PID to be signalled. A socket lookup misses a server still starting and a stale socket survives exit; a shutdown API also misses startup. Stop need not use either manager because both restart only on failure. Connect starts an installed service rather than compete with it; restart is stop plus connect. macOS launchd behaviour requires Mac verification.

Personalization uses the starter pack's existing settings skill: when the person asks, the agent creates or edits `$RAZOR_HOME/about.md` in their words. Setup leaves an existing file alone and creates none. A separate profile command would duplicate an operation the skill already owns.

Keys stay in `$RAZOR_HOME/keys.toml` at 0600 because a background service does not inherit the login shell; a keychain would add platform APIs and Linux session-unlock requirements. Model sign-ins stay in their providers' existing stores; model keys join the web and voice keys there, under any variable's name, and `~/.razor/key` is gone.

No new dependency or config key is needed; the PID is written into the existing server.lock file. Existing environment overrides, provider auth storage, key validation and the owner-only home rule remain authoritative.

The obligations are home isolation, a stop and restart that hold through shutdown, recovery commands that address their causes, and command help with no effects. The server lock is a flock: an fcntl lock is released when any descriptor of the file closes in the process, and the GPT path's file tools run in the server. A small race remains between stop's lock recheck and its signal, shared by every PID-based stop. The CLI keeps an inherited `RAZOR_SESSION` only when that record exists in the selected home; an inherited resident id can still attach to a same-named resident in another home, an accepted residual that does not justify a second variable in agent shells. Provider-owned sign-in may create its own directory, so doctor promises to start no server rather than to write nothing. Measured counts are in SPEC §6.
