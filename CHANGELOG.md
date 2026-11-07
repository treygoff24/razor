# Changelog

What changed in each Razor release. [Configuration](docs/configuration.md) and the other pages in [docs](docs/README.md) describe each feature in full.

## 0.2.1 (6 November 2026)

### New

- Models behind any API key. A `[providers.<name>]` table says where a provider's models are reached (`format`, `base_url`, `key`, `models`), and its models are named `<name>/<id>`. Keys go in `~/.razor/keys.toml`, which only you can read, and agents' shells never see them. `razor setup` offers an API key beside the Claude and ChatGPT sign-ins, so a fresh install no longer needs a subscription, and `razor doctor` sends your default model one short test request ([Providers](docs/configuration.md#providers)).
- `provider_only = true` keeps Razor itself to the providers you named: no sign-ins, pairing, automatic updates, Razor's own web and voice services, or MCP servers a project's files bring. MCP servers you add yourself stay, and so does `razor update` when you type it ([Security](docs/security.md#contacting-only-your-model-provider)).
- A top-level agent can start a new top-level session for you, on this machine or on a paired server, with the model and effort it chooses.
- Image generation is on by default with a ChatGPT sign-in, and agents on other models are told to hand image work to a GPT helper. `[profiles.codex] image_gen = false` turns it off.
- The `razor` starter skill, replacing `razor-settings`: agents use it to change Razor's settings for you, add API keys and providers, add MCP servers, tools and skills, pair machines and phones, and diagnose and fix Razor itself.
- A new agents overview: agents that need you come first, each working agent gets a tile showing what it is doing now, and helpers sit under their parent. The agents list beside the transcript is redrawn to match.
- Scripts and apps can follow a session's events with `follow=true` without counting as you looking at it, so a dashboard watching every agent does not silence finished-turn notifications ([Scripting](docs/scripting.md)).

### Changed

- **Moving from 0.2.0 with your own model rows or an OpenAI key file.** A `[models."…"]` row no longer holds `base_url`, `key` or `id`: put them in a `[providers.<name>]` table as `base_url`, `key` and `models`, and name the model `<name>/<id>`. `~/.razor/key` is no longer read: put that key in `~/.razor/keys.toml` as `OPENAI_API_KEY = "…"` (`chmod 600` the file) and use the model `openai/<id>`. A settings file with an old row is refused and the server will not start until you move it; the refusal and `razor doctor` say what to change. Run `razor doctor` after updating.
- Background commands and `monitor` jobs no longer stop after 30 minutes or two hours: they run until they finish or are stopped, unless the agent sets a timeout. A foreground command that outlives its wait keeps running in the background.
- Catch-up summaries are written by the session's own model, through its own sign-in or provider, instead of always through Claude.
- Live calls are quieter. The voice speaks at a relaxed pace and passes on news that matters rather than every step. When the line goes quiet it says the agent is still working after a minute, not after 7 seconds, and an idle call ends after 15 minutes instead of 3.
- In the agents list, Space folds and unfolds a parent's helpers; the clickable fold mark is gone.
- Agents operate desktop apps with Cua Driver instead of agent-desktop. Razor ships Cua Driver 0.40.6 and runs its background program for agents on a Mac where Razor has Accessibility, or on Linux when the server has a display; it stops with the server. Razor turns off Cua Driver's usage reporting and update check. Cua Driver tries to work without taking over your screen but does not always manage it, so agents are told to say so before a desktop task. On a Mac, Cua Driver keeps the display awake until ten minutes after an agent's last desktop call, because it cannot work while the Mac is locked. The starter skill `cua-driver` replaces `agent-desktop`, and `razor doctor`'s Desktop line says whether the background program answers ([Browser, voice and search](docs/browser-voice-search.md#desktop-apps)).
- Every release ships the latest stable agent-browser and the newest Cua Driver release published before it, though every Cua Driver release is marked a prerelease; this one has agent-browser 0.41.2 and Cua Driver 0.40.6.
- A server waiting for the answer to its own question no longer counts as you looking at the session.

### Fixed

- A session with a long call or meeting recording no longer shows a blank transcript with only "earlier items" until you scroll up, and a followed transcript keeps drawing new replies past the meeting's lines.
- Dragging the agents list's edge works again where a row used to cover it.
- Attached images no longer show their file path in brackets in your view of the conversation.
- The new-agent chooser finds every project and names it by its folder.

## 0.2.0 (6 November 2026)

### New

- `razor update` installs the latest release. A release install also checks when its server starts and once a day, installs a new release in the background without interrupting running sessions, and shows a one-line notice to run `razor restart`; `release_updates = false` turns the background check off. An install of 0.1.0 updates once with the install one-liner first.
- Large images you attach are resized to fit the model's limit instead of being refused; your original stays on your machine.
- Agents can operate desktop apps. agent-desktop now ships with Razor beside agent-browser and reads and operates apps through the system's accessibility interface, and Razor's prompt now tells agents about both programs. On a Mac, `razor doctor` offers to ask macOS for the Accessibility and Screen Recording permissions; nothing is on until you turn Razor on in System Settings ([Browser, voice and search](docs/browser-voice-search.md#desktop-apps)).
- Pictures in the transcript: the ones you attach, and the ones Razor's own tools return to an agent, such as an image file a GPT agent reads, a generated image or an MCP server's screenshot. They are sharp in terminals that speak the Kitty graphics protocol, such as Kitty, Ghostty and recent iTerm2, and drawn in coloured blocks elsewhere.
- A full-screen overview of every agent: what each is for, what it is doing now, any question it is waiting on, and its helpers. Press Left twice at the empty composer, or F3.
- Go to (`/goto`): find any agent on any connected machine, or search your past conversations, and open it.
- Keys in the style of tmux: with an empty draft, press backtick or Ctrl-B, then a key, to switch projects and agents, open a helper's split, rename or close a session, or step through replies to quote, copy or read one in your own editor. The prefix and `?` list every key. Left and Right move between a session, the agents list and the overview; panels scroll with Vim keys, and `/` searches `/keys`, `/help` and `/tasks` ([Terminal keys](docs/configuration.md#terminal-keys)).
- With words in the draft, Ctrl-C empties it and undo brings them back; Ctrl-S sets the draft aside and brings it back later.
- Suggested next messages, off by default: with `[ui] suggestions = true`, a reply may end with a suggestion for your next message, which Tab puts in your draft. It uses a few extra tokens and no extra request.
- `razor config get`, `set`, `unset` and `path` read and change settings from the shell ([Change a setting](docs/configuration.md#change-a-setting)).
- Simpler pairing over Tailscale: `razor setup` fills in this server's name and Tailscale address, `razor devices find` lists the other Razor servers on your tailnet with the command that pairs each, and the other machine confirms with one command ([More machines](docs/machines.md)).
- A terminal without a local server: a `[remote]` section lets `razor` on one machine open sessions that live on another ([More machines](docs/machines.md#use-a-terminal-without-a-local-server)).
- The new-agent chooser lists your projects, including the folders you have worked in on paired servers.
- Your own skills in `~/.agents/skills`, `~/.claude/skills` and `~/.codex/skills`, where Codex and Claude Code keep them, are available to every agent.
- Installed skills update daily: clean Git checkouts move forward, skills from the `skills` command-line installer are reinstalled, and starter skills that follow an upstream project get its newer copies. `skill_updates = false` turns this off ([The system prompt and skills](docs/configuration.md#the-system-prompt-and-skills)).
- Sixteen more starter skills: for designing, changing, repairing and reviewing code, questioning a design, honest reporting, writing documents for agents, questioning a plan, improving a measured number, review by another model family, prompting GPT, writing an agent's persona, Claude Code goals, frontend design (impeccable), desktop apps, and setting up always-on agents.
- Agents are reminded to bring tasks, decisions and the project's state note up to date before an idle handover and after two helpers report back.
- `/background` and `/exit` leave the terminal app as `/detach` does, with the session still running, to resume later; `/end` still ends it. `[ui] quit = "end"` makes Ctrl-C twice end the session instead of leaving it running.

### Changed

- Every release ships the latest stable agent-browser and agent-desktop; this one has agent-browser 0.41.2 and agent-desktop 1.2.0.
- Each agent gets a browser of its own without naming one: Razor sets `AGENT_BROWSER_SESSION` to the session's id in every agent's shell.
- The agents list shows each agent's goal and the command or tool it is running, indents helpers under their parents, and has a ‹ at its top right that closes it.
- A settings file that names an unknown setting, such as a misspelled key, is now refused whole, like one that does not parse. A running server keeps that file's last good settings and names the problem in the status pane and `server.log`; a server whose own file is refused does not start. Run `razor doctor` after updating to check that your settings load.
- `[ui] blocks` defaults to `"auto"`, so the workshop scene and the clock use half blocks inside tmux, whose width tables can garble octants.
- The terminal key action `line_start.list` is now `line_start.outward`; a setting with the old name is refused.
- A `[claude] sign_in` command now runs with the conversation's session id in `RAZOR_SESSION`, so it can choose an account per session.
- `razor keys` and the docs name the permissions a restricted ElevenLabs key needs: Text to Speech and Voices (read).

### Fixed

- A GPT conversation that grows past the request size limit rebuilds its context from the record instead of stopping, and long conversations with many pictures send only the newest pictures, leaving older ones on disk for the agent to read again.
- When Claude refuses an account because its organization turned off subscription access, that conversation's sign-in is dropped and the next turn signs in again, without a server restart.
- Over SSH, read-aloud says the audio would play on the other machine instead of playing nothing silently.
- The speech program explains a Python without working SSL support and gives the setting that fixes it.
- After a read-aloud failure, Ctrl-Shift-C copies the error, and a message typed afterwards sends normally.
- `razor setup` reads your answer correctly when an SSH terminal sends stray escape sequences or backspaces.
- Typing `/` in a brand-new session lists its skills before the first message.
- `razor --print` no longer misses the first words of a fast reply.
- Installing an update moves the link to the current version in one step, so a `razor` command run during the install never finds the link missing.
- A terminal app whose settings fail to load starts the local server, which writes the reason to `server.log`.

## 0.1.0 (5 November 2026)

The first public release, for Linux (x86_64 and arm64) and Macs with Apple silicon.

### New

- An agent harness that runs Claude through your Claude subscription or an Anthropic Console account, GPT through your ChatGPT sign-in or an OpenAI API key, and any model behind an OpenAI-compatible chat endpoint.
- Conversations run on a server, so they keep going after you close the terminal, can be watched and steered from another terminal or machine, and reopen with `razor --continue` or `razor --resume`.
- When a conversation's context fills, the agent writes a short note and continues in a fresh context with your latest requests word for word; everything earlier stays in the record for it to look up.
- Helpers that work on their own git branches and report back, then are merged or discarded.
- Always-on agents that wake on a schedule in their own folders, with their own memory.
- Memory for each project: a note on where the work stands, your decisions in your own words, facts and tasks.
- Voice: replies read aloud, voice notes you record, and short spoken notes from agents.
- A real browser through agent-browser, which ships with Razor, and web search through Exa, Parallel or Firecrawl.
- Pairing the Razor servers on several machines over Tailscale, with one list of every agent.
- MCP servers, your own scripts as tools, plugins that can change or refuse what agents do, and `razor --print` and a local API for scripts.
