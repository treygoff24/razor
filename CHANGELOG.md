# Changelog

What changed in each Razor release. [Configuration](docs/configuration.md) and the other pages in [docs](docs/README.md) describe each feature in full.

## 0.4.0 (8 November 2026)

### New

- Dreaming: each night at 03:00 Razor checks a project's memory against its recent sessions and sources, one project at a time, in a fresh session. It corrects or retires what no longer holds, files lasting knowledge into wiki pages, merges the sessions' state notes, and looks for repeated failures and questions. It can read, recall, load skills, read status, use memory and send messages, but has no shell or file-writing tools, and it asks you at most one question per project per night. `razor dream [folder]` runs it now; `[memory] night` sets the time or, with `false`, turns it off, and `[memory] model` picks its model ([Memory](docs/memory.md#dreaming)).
- Wiki pages: dreaming keeps a project's lasting knowledge in `wiki/` under its memory folder (`all/wiki/` for every project), each page with a name, a line saying when to read it and the date it was last checked. The startup page lists them, and search and `get` read them as `wiki/<name>`. A `wiki/_instructions.md` you write tells dreaming what matters and is never changed by it ([Memory](docs/memory.md#wiki-pages)).
- Each session keeps its own state note, so sessions working in parallel no longer overwrite each other's handoff; the startup page shows the merged note and then newer sessions' notes, and a session's note expires after 14 days unless it says otherwise.
- Facts and notes can cite their sources with `cites`: session entries, files at a commit, cached references, fetched pages and tasks. Dreaming checks cited code against main and live branches ([Memory](docs/memory.md#sources)).
- A project can share its memory across machines through `[memory] remote` in `.razor/config.local.toml`, carried on its own git reference the way tasks are ([Memory](docs/memory.md#several-machines)).
- `recall` with only a session gives that session's digest, and `GET /v1/sessions/<id>/digest` gives it to programs: at most 24 KiB of its inputs, replies, failed calls, memory and task calls and changed files, newest failures first ([Scripting](docs/scripting.md#the-local-api)).

### Changed

- An always-on agent's night asks it only to close finished tasks, turn promises into tasks with due dates and write its state note; dreaming does the memory tidying.
- **For scripts and apps reading the record:** a skill handed with a message is now its own field, `skill: {name, text}`, on the input, and the input's `text` holds only your words. Older records keep the skill inside `text`. The phone and Mac app read the new field from their next update; until then they show a skill handed back with no words as an empty message.
- Handing a skill back to an agent no longer touches that agent's saved draft. Only a message carrying your words, pictures or files clears it.
- `/done` and `/quit` clear the command from the composer as soon as they are sent. If you type more before the session closes, the terminal stays on the closed session with your words, and sending them reopens it.
- Changing the key setting of a Claude-format provider takes effect on the next turn of an open conversation, instead of only once the conversation is reopened.

### Fixed

- A patch that moved a file to its own path deleted it.
- Opening the terminal with `--agents` could switch to your most recent conversation after you had started typing a new one.
- Taking a queued message back into the composer while the terminal's writes were backed up copied its words without withdrawing it, so it could still be sent.
- Clicking a place in a conversation's history while an earlier page was loading could lose the jump.
- A picture pasted into a new agent's draft could attach to another draft if you switched destinations while it loaded; switching now waits for the picture.
- A slow Earlier page could overwrite a newer one after folding and reopening Earlier.
- A helper that reported and was then continued kept reading "ended" in the stage strip; and a failed call could be marked fixed by a matching call in a later turn.
- A phone's Live Activity could miss an update or its end when many were waiting to go out; they now wait their turn instead.
- An always-on agent's run that waited more than a day showed Waiting while it ran.
- A helper waiting out its grace after sending mail lost the grace, and the reminder that follows it, if the server restarted.
- A closed session's task claims stay owed until releasing them succeeds, instead of being given up after one failed read; a task file that cannot be read is a refusal rather than an empty list.
- Replacing a memory writes the new one before retiring the old, so a failed write no longer leaves neither active.
- A code cell whose final status could not be saved no longer reports itself running forever.
- The server writes `server.json` whole, so a reader never sees half of it.

## 0.3.1 (8 November 2026)

### Changed

- A paired device that sends a scaled picture may leave its origin's `device` empty, and the server fills in the name the device was paired under, so the record says which device keeps the original ("IMG_1234.HEIC in Photos on phone"). An origin that names a device keeps it. The `image_origins` field is otherwise unchanged.
- Releases ship Cua Driver 0.44.0, Cua's newest, in place of 0.43.2: it types exact text on the active keyboard layout or refuses it, names the dialog that blocks keys, and reports which window an action opened on macOS.

## 0.3.0 (8 November 2026)

### New

- Tasks in the terminal app. The status pane's TASKS section shows the project of the session on screen: the tasks that session holds, then overdue ones, then the rest, with `▸ N more · ✓ k done this session` opening every open task fifteen to a page and then the ones this session closed. `/tasks` groups the project's tasks under held by this session, ready, in progress, blocked, with open parts, and done this session; a click on a task or `/tasks <number>` opens its card, with its blockers, parts, description and newest comments, and Esc goes back. Both follow changes from any session or paired server as they land ([Memory](docs/memory.md#tasks)).
- Programs read tasks as a session sees them ([Scripting](docs/scripting.md#the-local-api)). `GET /v1/sessions/<id>/tasks` lists the tasks that session holds first, marks each `held`, names the claiming session as `claimant` and the closer as `closed_by`, gives the project's folder as `project` and, with `filter=open`, `counts` by state, and takes `q` for words, `limit` up to 200 and, with `filter=closed`, `mine=true` for the session's own closes. `GET /v1/sessions/<id>/tasks/<n>` gives one task whole, and the status stream sends `{"kind":"tasks","project":"<folder>"}` when a project's tasks change, here or in what a fetch of its shared list brought, and not for a read that changed nothing.
- A Mac's server says it is going to sleep, holding sleep back a second for the line to go out, and its paired servers list it as not answering at once, instead of when their reads time out about 25 seconds later; when the Mac wakes they list it answering again. The Mac's server and its peers all need this release. The status stream's `server` line carries it as `silent` ([More machines](docs/machines.md#what-pairing-gives-you)).

### Changed

- The server no longer keeps a row in memory for every agent it has seen. It keeps the live agents list (agents at work, asking or stuck, idle ones with an unread delivery, your message in the last twelve hours, a terminal open on them or a reply you have not looked at, their helpers and the always-on agents) and the sessions it has loaded, and reads the rest from disk when they are asked for, so its memory no longer grows with your history. In a test, a restart that found 1,000 recent closed conversations used to keep a row for each; it now keeps none.
- Earlier, in the agents list, opens without a total and reads up to 50 top-level agents at a time from each server, showing each server's as it answers, so a slow one holds up only its own; More reads older ones, a failed read says which server and offers Retry, and folding Earlier lets them go. Its count is of the rows it has read. A helper of an agent under Earlier is seen in that agent's conversation instead of beneath it in the list.
- A helper whose parent is not in the agents list stands in the list by its own state, and moves back under its parent when the parent is shown.
- **For scripts and apps reading the status stream:** its `agents` line holds the live list only, an `agent` line changes a row or adds one that joins it, and a whole new `agents` line replaces the list when one leaves. Older agents are read a page at a time from the new `GET /v1/agents?after=<session>`. Rows carry `changed_file_count` and `changed_file_first` instead of the full `changed_files` list, which `GET /v1/sessions/<id>/agent` gives with the session's row and `children`, a row for every direct helper, settled ones included; `GET /v1/sessions/<id>/children` is gone ([Scripting](docs/scripting.md#the-local-api)).
- Mail by name reaches an agent in the live list, an always-on agent or one in the sender's own tree; an agent under Earlier is reached by its session id, which also wakes it if it has not ended. `message list` lists the same agents.
- An agent's `status` of its tree gives a helper on a paired server the files it changed as that server records them, or `null` when that server does not answer, as one older than this release does not. A helper's card reads its model and effort from the server it runs on when the list does not carry them, and leaves them out while they are unknown.
- `/` no longer searches `/tasks`; it still searches `/keys` and `/help`.
- Releases ship Cua Driver 0.43.2, Cua's newest, in place of 0.41.3: it types text faster by sending key events in batches, retries reading the screen when its extension's lock is busy, and cleans up extension files a refused install left behind.

### Fixed

- A session started for you without a first message, such as one made with `POST /v1/sessions`, stays in the live agents list for twelve hours, as one you had written in does, instead of going under Earlier while idle.
- A message the server took but whose answer never reached the terminal, because the connection dropped or a paired server failed partway, is no longer sent twice. The terminal keeps it out of the box and says what failed; Enter or the Send button sends the same message again, and the server, which recognises it, keeps one.
- A draft save the server answers slowly can no longer land after a newer one and put your older words back, nor lose the newer words when you move to another agent: the terminal waits for each save's answer instead of giving up after 15 seconds, and saves after it only once it has. While eight saves wait on a slow server, the terminal no longer keeps a processor core busy.
- A draft save the server was still writing when its connection dropped can no longer finish after a newer save and put the older words back: the server writes drafts, and clears them when it takes a message, one at a time in the order they reached it.
- Quitting while a message waits unanswered now says where its words go, and keeps them: they are saved as the draft ahead of anything typed since, even when you sent before the terminal had read that session's draft. If the server got them, the transcript shows them, so send them again only if it doesn't.

## 0.2.2 (7 November 2026)

### New

- Keys for moving between agents and typing to each one from the keyboard alone, with the agents list staying open. They are new actions, unbound by default, so the default keys are unchanged: `choose` at the start of a composer line and `switch` anywhere in the draft give the list the keys with your current agent selected; in the list, `previous_agent` and `next_agent` step through the top-level agents, passing over helpers unless one needs you, `beside` opens the selected agent beside the list, ready to type, and `overview` opens the overview. [Moving between agents](docs/configuration.md#moving-between-agents) gives the settings that bind them to Left, Shift-Tab, the arrows and Enter.
- The `razor` starter skill tells agents how to bring your MCP servers, skills, plugins and tools over from Claude Code, Codex, OpenCode, pi, Code Puppy or any other agent: what `razor mcp import` reads and what it leaves for you, where each agent keeps its setup, and what to do with a plugin, hook or tool that runs only inside the other agent.
- `razor config get`, `set` and `unset` reach the voice settings the server keeps, by their names after `voice.`, such as `razor config set voice.quiet '{"from": "22:00", "to": "07:00"}'` ([Browser, voice and search](docs/browser-voice-search.md#spoken-notes-from-agents)).
- A session's event stream sends `{"kind":"heartbeat"}` after every 15 seconds without a line when asked with `heartbeat=true`, so an app can tell a quiet session from a dropped connection ([Scripting](docs/scripting.md)).
- The status stream's `server` object says `"follow":true`, so an app can tell that this server honours `follow=true` on a session's events, which follows the session without marking it looked at. An older server ignores that query, marks the session, and leaves `follow` out.

### Changed

- Models that hold 500,000 tokens or more, Claude Code's Opus, Sonnet and Fable at 1,000,000 among them, now fill at most 500,000 and hand over at 450,000 (a threshold of 0.9); smaller models keep their own window and 0.8. A `window` or `threshold` you set for a model under `[models]`, or a `[rebuild] threshold`, still wins ([Configuration](docs/configuration.md#models)).
- Voice notes and phone calls have no daily limit unless you set one. The old defaults, 3,000 spoken characters and $10 of calls a day, are gone, including from settings that saved them: an exact 3,000 or $10 saved before this release is cleared as the old default, and any other limit stays. With paired servers, every server must update before calls have no limit, since an older one still applies its own $10 to the calls it makes.
- Agents' voice notes have no length limit, and the `memos` voice setting is gone; a note still fits on one line, and counts against `daily_characters` if you set one. The bundled speech program speaks a long reply or voice note whole, instead of stopping at 5,000 characters with "The rest is in the app."
- A working helper's mark in the agents list and the overview is the green of a working agent, instead of its group's muted colour, so helpers at work stand out.
- When an agent sets its own session's title or name, the transcript labels that call `title` or `rename` in the ordinary colour, instead of the violet `agent` used for starting a helper.
- A server reached over the network lets go of a phone or other device that vanished without closing its connection: on Linux about two minutes after the device last answered, instead of hours later; on macOS an idle connection closes the same way, and a streaming one once macOS gives up retransmitting.
- Releases ship Cua Driver 0.41.3, Cua's newest, in place of 0.40.6: its fixes keep a macOS sheet's own focus, restore the previous foreground after typing on Linux and Windows, and decide every macOS activation undo in one place.

### Fixed

- Unsent words are no longer lost when you move between agents. Leaving an agent before its saved draft has loaded keeps that draft, and words and images you type before it loads go after it when it arrives; until then you stay on that agent. Moving away waits until the server has saved your words, and if it cannot, because eight saves are already waiting or the save fails, you stay with your words and the reason. Sending before the draft loads keeps the draft too, so `razor --resume <id> "words"` no longer discards it.
- Words typed in the half second before Right closes the agents list are no longer lost.
- When a refused message comes back to the box, the words and images you typed meanwhile stay with it, even past 1 MiB together, until you shorten them, instead of the new words going to the clipboard. If you had pasted images meanwhile, the returned message's own images are dropped, and the terminal says so; it also says so when you leave a conversation with images in the box, since a draft keeps words only.
- Words typed for a new agent or thread are no longer lost when you look at another session: they wait, for up to eight new agents at once, and come back when you return to start that same one. Leaving words for a ninth, or words past 1 MiB, is refused with a sentence until you send, stash, clear or shorten them. The first Ctrl-C of a quit names the new agents and threads whose words would be lost, and `/new` or `/thread` back to one keeps them; closing the terminal window still loses them.
- The terminal no longer moves you to another session when one you closed with `/done` or `/quit` finishes closing, if you had gone on to another session or started typing for a new agent or thread meanwhile.
- A view grouped by state, the default one included, lists an agent that asks or is stuck once, under NEEDS YOU with its helpers beneath it, instead of again in an Asking or Stuck group below; the overview shows those agents as its first panel.
- Ctrl-N and Ctrl-P select the helpers of an agent opened beside the agents list, instead of saying there is no other child.
- The agents list's last line no longer says Left opens the overview, since Left there moves to the parent row; it names F3, and the keys of whichever list actions are bound.
- The status stream always opens with its `agents` line, as [Scripting](docs/scripting.md) says; a line about a paired server's agent or a delivery could come first, so a script reading only the first line sometimes found no agents in it.
- A question from an agent with a blank line in it no longer crashes the terminal app.
- The context gauge, folded to eight columns, shows a rebuild threshold such as 0.57 as 57 instead of "56.".
- `razor --agents` opens beside the conversation you last wrote in, instead of whichever agent last did any work, which could be someone else's busy agent.
- When a session you closed with `/done` or `/quit` beside the agents list finishes closing, a helper goes back to its parent and any other session to the next top-level agent in the list, never to another agent's helper; with none, the closed session stays on screen. This also works when the list has already moved the closed session to Earlier, which used to send you to the top of the list.
- Quitting the terminal beside the agents list names the session on screen in its last line, instead of one you had opened earlier.
- The second and later lines of a quit warning, such as the one naming a new agent's unsent words, start under its first line instead of a column to the left.
- Opening a session from Go to unfolds what hides it in the agents list, Earlier included for a closed one, and selects its row, instead of leaving no row selected.
- `razor config get voice.<field>` on a server whose `[server] shared` names another reads that server's value, and `set` and `unset` there say which server keeps the voice settings, instead of "Use the voice settings owner shown in status."
- A panel's last line, such as `/help`'s or `/keys`'s, and the Go to and `/mcp` panels' leave out the hint of a key your settings unbind, instead of saying "unbound".
- The agents list's servers line names whose usage window it shows, as in `work 7-day 97%`, since it is the fullest window on any paired server and may not be the account your conversation uses.
- Each reply's name line in the transcript says the model that wrote it. Switching models no longer relabels every earlier reply with the new model, and words sent with a model change that join a running turn no longer relabel that turn's replies; replies from before this release still show the session's current model.
- Claude agents no longer take the line Claude Code adds after reading a large image, `[Image: original …, displayed at …]`, for a message from you: their prompt says it is the tool's scale note.
- A helper whose turn failed or was interrupted reads `▲ error` under its parent in the agents list and wears the red `▲` in its parent's overview tile, as its own row does, instead of `idle`. An agent that has finished or closed no longer shows as an error because its last turn failed.
- With motion off, or the terminal in the background, an age under a minute in the agents list, the overview and the status pane's inbox reads `<1m` and changes at the minute, instead of counting seconds and redrawing the screen every second.

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
