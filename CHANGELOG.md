# Changelog

What changed in each Razor release. [Configuration](docs/configuration.md) and the other pages in [docs](docs/README.md) describe each feature in full.

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
