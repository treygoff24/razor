# What a stranger gets out of the box

Four parts: the starter pack a fresh install uses, notifications that need no program of the person's own, agent-browser at runtime, and the Cua Driver daemon the server runs for desktop apps.

## Surface

```rust
// src/config/pack.rs, inside `parse`: in `ask`, `notify`, `speech`, `transcribe`, `capture` and
// `play`, a program named by a path containing `/` counts from its file's folder (`~/` from the
// home folder), as every other path in a settings file does; a bare name is still found on PATH.

// src/profiles/startup.rs
/// As before, plus one word: `about` is the person's `about.md` in Razor's `home`, which
/// `razor setup` writes; a missing one is left out without a line in the facts.
pub async fn opening(profile: &Profile, list: &[String], root: Option<&Path>, s: &Summary,
                     identity: Option<String>, home: &Path) -> Opening;
```

`src/api/speech.rs`: a reply whose speech program fails is refused with `Couldn't read it aloud:` and the program's own sentence, in place of "see the server log". `scripts/speech.py`: with no `ELEVENLABS_API_KEY` that sentence is `speech: there is no ElevenLabs key; run razor setup to add one.`

`scripts/notify.sh`: the starter's `notify` and `ask` program. Its one argument becomes a desktop notification titled "Razor", through `osascript` on macOS or `notify-send` on Linux, with `--` before the text so no character of it reads as an option; where neither program exists it exits 0 and says nothing. It prints nothing, so an ask it carries waits for a client's answer.

`src/config/pack.rs`, `Settings`: a person's file that names no `pack` gets `packs/starter` from `config::INSTALL_DIR` (the one resolution of the binary's folder, made before the runtime starts), else from the source tree the binary was built from, read and layered as any pack (docs/design/packs.md); the lookup is a stat in Settings' own blocking read. A file that names a pack gets that pack alone, and removing the name brings the starter back. With neither folder holding one, there is no pack and no error.

**The starter pack** (`packs/starter/`, text, besides the scripts two skills run):

| File | What it holds |
| --- | --- |
| `config.toml` | `startup = ["persona.md", "razor", "about", "project", "rules.md", "code-quality.md"]`, with the instruction that a person who changes it copies the list and the files it names beside their own file, since a relative path counts from the file naming it; `notify` and `ask` set to `../../scripts/notify.sh`; `speech` set to `../../scripts/speech.py`; `[web]` listing Exa, Parallel and Firecrawl, each verb live only while one of its keys is set (`src/tools/web/mod.rs:83-94`), the model's own web tools otherwise. Every other optional feature is a commented example with one sentence: ntfy for a phone, `play` and `capture` for each platform, model and effort, `memorum`, guards, children, a local chat model, an MCP server, a resident with a schedule, a plugin, the terminal's motion. Each example parses when uncommented. |
| `persona.md` | Who the agent is to the person: a collaborator who brings judgment and says when it disagrees, in general terms, with nothing personal. |
| `rules.md` | How it talks and reports, kept apart from what Razor's own prompt already says. |
| `code-quality.md` | Seven lines on how it writes code: deep modules, each fact in one place, a named reason for every new mechanism, deleting before adding. In a blind-reviewed benchmark (7 Nov 2026, three Opus runs a setup) it brought Razor's code-quality scores level with Claude Code's, from lowest, at the same cost. |
| `skills/agent-browser/` | A self-contained guide: every command from agent-browser's own documentation, run by hand against v0.41.0, plus a browser per agent, named by the `AGENT_BROWSER_SESSION` that `Tools::open` sets to the session's id beside `RAZOR_SESSION` (the unnamed one is shared by every agent on the machine), covered clicks and `doctor`, less its date-field line, whose `.value` assignment fires neither `input` nor `change`. It credits agent-browser's documentation (Apache-2.0, 2025 Vercel Inc.). There is no pointer to `agent-browser skills`, which the native binary Razor ships lacks. |
| `skills/cua-driver/` | Our own short guide to the Cua Driver CLI, written from Cua's guide for the shipped release: every call with `--socket "$CUA_DRIVER_SOCKET"`, element tokens, `type_text` for text fields (on Linux `set_value` on a GTK text field also presses the dialog's default button), the controls that keep it off the person's screen, and never starting, stopping or updating it. Cua's own skill teaches `update --apply` and runs to 13 KB plus 275 KB of references; `get_guide` serves those references on demand. |
| `skills/razor/` | Setting up, changing and troubleshooting Razor: settings, providers, keys, tools, devices and updates, with references for each job. |
| `skills/writing/`, `skills/review/` | Prose a person will read; reviewing a change. |
| `skills/changing-code/`, `repair/`, `task-design/`, `structural-review/`, `challenge-the-design/` | Changing code: the standard and shared design words, then the procedure for the moment: fixing a bug, designing a change, reviewing structure, questioning a subsystem. |
| `skills/no-ceremony/`, `writing-for-agents/`, `grilling/`, `claude-goal/` | Honest reporting and process that earns its place; documents an agent reads; questioning a plan round by round; a Claude Code `/goal` condition. |
| `skills/hillclimb/`, `cross-family-review/`, `codex-prompting/`, `agent-persona/` | Improving a measured number with its `hc` harness; reviewing a finished package with helpers from the other model family; prompting GPT models; writing an agent's persona. |
| `skills/impeccable/` | Frontend design, copied unmodified from its upstream at the revision its `UPSTREAM` file names, with its license; the NOTICE credits it. |
| `skills/new-steward/` | Setting up a steward, a resident that decides a project's design, product and size-limit questions for the person: the interview, a charter template, the resident's settings and the paragraph that routes other agents' questions to it. |
| `skills/residents/` | Choosing and setting up an always-on role: a short interview, the working folder and memory conventions, four worked configs, contact and access policy, and a trial run. |

**The about-you file**, shared with `razor setup` (docs/design/setup.md): `$RAZOR_HOME/about.md`, plain Markdown, first line `# About <name>`, then free lines. Setup writes it, never inside the pack. A pack cannot name it by path: a pack's relative paths count from the pack and `~/` from the home folder (`parse`), and neither reaches `$RAZOR_HOME` when it is not `~/.razor`. Hence the word.

## Data flow

1. The person's file names no pack, so Settings reads `<install>/packs/starter` (a release) or `<checkout>/packs/starter` (built from source).
2. The starter's `config.toml` sits beneath the person's file; its relative paths count from the pack, so `../../scripts/` is the bundled programs in a checkout and in a release, which keeps the source layout.
3. A conversation opens with persona.md, Razor's own text, `about.md` when it exists, the project's files, then rules.md and code-quality.md.
4. Its first input lists the starter's skills with descriptions. Razor's own text (`data/razor.md`) also names agent-browser and `cua-driver`, the programs that ship beside the binary, says when to use each and points to their skills, since the skill list alone did not make agents aware of them (Trey, 2026-11-06: "update the Razor system prompt to let agents know about agent browser and agent desktop, because I don't think they're aware of it right now, that it comes in the harness").
5. Web verbs appear for each verb with a provider key in the server's environment (setup's `keys.toml`); without keys the model keeps its own web tools.
6. Speech runs the bundled program; with no key the reply's ▶ and an agent's voice note are refused with the setup sentence, as an install with no `speech` refuses today.
7. A root's note to the person goes to the inbox, then through `notify` to the desktop.
8. A root's question goes through `ask` to the same notification; the script prints nothing, so a client answers.
9. A headless Linux server has neither notifier (libnotify is a desktop package): the script exits quietly; the person reaches a phone by copying the ntfy lines, which send through `curl`, already an install requirement.
10. A person's file that names another pack gets that pack alone: no starter, and only the startup words and programs that pack sets.

## The hard decision: what fires with no configuration

| | Desktop, always | Only when no terminal is attached | Nothing until setup turns it on |
| --- | --- | --- | --- |
| A question while the person is away | banner | none, if a terminal was left open on that session | none |
| The person watching the session | banner beside the question band | none | none |
| Headless Linux | silent | silent | silent |
| Cost | a configured script; no Razor code | the ask and note paths would look up the session's attached terminals; a resident's failure note has no session the person attaches to | a feature setting setup does not write |

**Chosen: desktop, always, as the starter's configuration.** "Attached" is not "looking": a terminal left open in a background window or overnight would silence every notification, at exactly the time the person needs one. The status row's terminal count is the only attachment signal on Linux, and the activity samples the voice code uses come only from the macOS terminal. Nothing-until-setup cannot work, since setup writes keys and no feature settings, so a stranger's first agent that asks a question would wait unseen. A missed question stalls an agent for hours; a duplicate banner costs a glance. On macOS the banners come from Script Editor, which the person may have to allow once; the starter says so.

This is configuration, not a fallback in code: Razor with no `notify` or `ask` still runs nothing, so `notify = []` and any other pack stay silent as before, and the residents module is unchanged. The phone needs no new key either: the ntfy lines are `curl` with `-o /dev/null`, without which curl's reply would become the ask's answer.

Every flow now runs on the starter, as a stranger's install does. Two assumed bare defaults and set the key they assume in their own config: flow 28 pins `startup = ["razor", "project"]`, since its 12,000-token limit is Razor's own GPT prefix and the starter's persona and rules (about 510 tokens) are configured startup text the spec lets grow it (12,022 estimated with them); flow 29 empties the `[web]` verbs whose absence it checks. On a Mac, eight flows (22, 26, 37, 47, 48, 51, 56, 57) would raise 21 desktop notifications through osascript; a no-op `osascript` and `notify-send` at the front of the flows' `PATH`, stand-ins for other people's programs, would keep them quiet without touching any flow's config.

## Cua Driver's daemon

Cua Driver's command line only talks to a running daemon of its own (`cua-driver <tool> '<json>'` refuses with "daemon is not running" otherwise), so the server runs one.

```rust
// src/residents/install.rs
/// The socket in a home on which `desktop` runs Cua Driver's daemon; serve names it to agents'
/// shells as CUA_DRIVER_SOCKET.
pub const DESKTOP_SOCKET: &str = "cua-driver.sock";
/// Runs the Cua Driver Razor ships as a daemon, as this server's direct child on `home`'s own
/// socket, only where it can work; again after an exit, except one within a minute of its start;
/// it ends with the server.
pub async fn desktop(home: PathBuf);

// src/config/setup.rs
/// The environment agents' shells, MCP servers and Cua Driver's daemon start from: this
/// process's, less Razor's own keys.
pub(crate) fn environment() -> BTreeMap<String, String>;
```

1. `main`, before the runtime starts and for every command, sets `CUA_DRIVER_SOCKET=<home>/cua-driver.sock`, `CUA_DRIVER_EMBEDDED=1`, `CUA_DRIVER_RS_TELEMETRY_ENABLED=false` and `CUA_DRIVER_RS_UPDATE_CHECK=false`, so every `cua-driver` Razor runs has them: the daemon and every agent shell through `environment`, and doctor's probe.
2. `serve` spawns `desktop`. It goes on only where the daemon can work: on macOS when `permissions` says Razor has Accessibility, elsewhere when `DISPLAY` is set; and only with a `cua-driver` shipped beside the binary (`config::shipped`), never one on `PATH`, so a person's own Cua install is never started by Razor.
3. It removes a socket file an earlier daemon left (the server holds `server.lock`, so none answers on it, and an embedded daemon refuses to start beside one), then starts `cua-driver serve --embedded --parent-liveness-stdio --socket <home>/cua-driver.sock --pid-file <home>/cua-driver.pid` as its direct child, with `environment`, its standard error in the server's log, and a pipe on its standard input that the server holds: the daemon exits when the pipe closes, which happens even when the server is killed.
4. An exit after a minute or more starts it again; an exit within a minute of its start ends desktop control until `razor restart`, with the exit in the server's log.
5. `razor doctor` runs `cua-driver --socket <home>/cua-driver.sock status`: found when the daemon answers.
6. Agents call `cua-driver --socket "$CUA_DRIVER_SOCKET" <tool> '<json>'`, as the skill and Razor's text say.

**Embedded and a direct child.** On macOS, Cua Driver by default re-launches itself as its own app (`com.trycua.driver`) so that its permissions belong to it. `--embedded` turns that off, so the daemon stays in Razor.app's chain and macOS answers its accessibility and screen checks with Razor's permissions; spawning it directly, never through `open`, keeps it there. Upstream's own embedding host passes the same arguments and two Razor leaves out: `--no-permissions-gate`, which embedded mode already implies, and `--host-bundle-id`, a label.

**Per home, not Cua's default socket.** Cua's default socket is per user (`~/Library/Caches/cua-driver/cua-driver.sock`, `~/.cache/cua-driver/cua-driver.sock`), so two homes would share one daemon, and a person's own Cua install would answer Razor's agents with its own identity and its telemetry on. The CLI reads no socket variable, hence `--socket "$CUA_DRIVER_SOCKET"` on every call.

**Where it can work, not always.** The daemon costs memory whether used or not; on a Mac without Accessibility or a Linux machine without a display it could do nothing. Starting it on first use instead would need a command or hook of Razor's for agents to call. Macs ask `permissions` at start for it, which loads ApplicationServices, under 1 MiB kept. Granting Accessibility later takes `razor restart`, which doctor already asks for, and which macOS needs anyway since it caches the answer per process.

**What it contacts.** Measured on Linux with Cua Driver 0.40.6, its daemon started and one `list_apps` call made with every proxy variable pointing at a local listener that logs each connection: without the two variables it connected to `eu.i.posthog.com` (six times), `cua.ai`, `api.github.com` and `github.com`; with them set, nothing. It wrote only its pid file in the home folder, which `--pid-file` moves into Razor's home. So under `provider_only` it stays on: it contacts nothing by itself. Its tools that download (`check_for_update`, `install_extension`, `install_ffmpeg`) run only when an agent calls them, as any command an agent runs.

**Memory.** On Linux under Xvfb, the idle daemon held 35 MB resident (21 MB anonymous) after start and 52 MB (36 MB anonymous) after a form task. On a Mac, idle, its physical footprint was 28.3 MB (87 MB resident, most of it shared system frameworks), 24.8 MB after launching an app and reading its window, and 46.4 MB (peak 48.4 MB) after typing a line into TextEdit in the background, which read it back with no change of the front app. It is a separate process, outside the server's idle memory limit.

**The Mac binary.** Cua's macOS binary is universal (81 MB). `release.sh` keeps its Apple silicon part with `lipo -thin arm64`; each part of a universal binary carries its own signature, so the thinned program keeps Cua's Developer ID signature and its entitlements (screen capture, Apple events), and Razor does not re-sign it. Checked on a Mac: `codesign --verify --strict` passes on the 39 MB result, whose code hash matches the universal binary's Apple silicon part, and Apple notarized the release archive holding it.

**Linux forms.** On GTK, Cua Driver's `set_value` on a text field also fires the field's activate action, which in a dialog presses the default button: one `set_value` on a yad form submitted it with that field alone (0.38 and 0.40.6). `type_text` with the field's element token types into it in the background and reads it back; by hand that filled the whole form exactly, and the skill says so.
