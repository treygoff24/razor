# Changelog

What changed in each Razor release. [Configuration](docs/configuration.md) and the other pages in [docs](docs/README.md) describe each feature in full.

## 0.8.0 (13 November 2026)

### New

- **For scripts and apps starting a sign-in:** `POST /v1/login` accepts `provider=claude`, `console` or `chatgpt`; `PUT /v1/login` sends a pasted code to the calling device's Claude sign-in. Omitting the provider keeps ChatGPT sign-in; older apps keep that behavior ([API design](docs/design/api.md#routes-under-v1)).
- `/login` offers Claude, ChatGPT and Anthropic Console. Claude sign-in stays in the terminal app, with a browser link and a field for its code; `razor login claude` or `razor login console` signs in from a shell without running setup again ([Models and sign-in](docs/models.md#claude)).

### Changed

- When an agent continues without answering your mid-work message, Razor holds one step of calls and asks it to reply first; the held attempts stay in the record without appearing as failed calls. Scripts already running finish their work, and a direct handover can proceed ([Concepts](docs/concepts.md#sessions-turns-and-states)).
- Sol's default context window now matches the 500,000 tokens advertised by its ChatGPT sign-in; Razor hands over at 90%. A manual `window = 500000` or `threshold = 0.9` for Sol now matches the defaults and can be removed ([Models and sign-in](docs/models.md#gpt)).
- Releases ship Cua Driver 0.49.5, Cua's newest, in place of 0.49.1: Mac screenshots release their memory, typing works in a minimized Screen Sharing window, Linux browser preparation works across languages, and a failed policy load exposes no tools ([Browser and desktop control](docs/browser-voice-search.md#desktop-apps)).
- **For scripts and apps reading the record:** `not_run` records an attempt withheld by the unanswered-message rule, with `call`, `name` and `input`, without counting it as a call or failure. A session holding one cannot be opened by a server on 0.7.0 or earlier; older terminals skip the entry and show their newer-server warning ([Using Razor from scripts](docs/scripting.md#withheld-calls-in-the-record)).
- **For scripts and apps reading the status stream:** the custom-avatar contract now documents image cells, owning-server names, helper inheritance, reloads and missing pictures. Older apps continue to draw their usual creatures ([Using Razor from scripts](docs/scripting.md#custom-avatars-on-the-status-stream)).
- **For apps using the Apple network package:** `NetworkClient.networkChanged()` looks for new paths after an app observes a network change; request timeouts log the phase and client activity in the system log ([Network design](docs/design/network.md#in-a-swift-app)).

### Fixed

- MCP resource links, embedded text and binary resource addresses now reach every model in the result's order; audio blocks report their type instead of disappearing ([MCP servers](docs/configuration.md#mcp-servers)).
- Answers to an agent's question still reach it after the question clears, and reopen a closed session ([Concepts](docs/concepts.md#questions-notes-and-deliveries)).
- Terminal retries deliver each answer once, even if the connection drops after the server accepted it ([Using Razor from scripts](docs/scripting.md#the-local-api)).
- An automatic close waits for all inputs to be answered, so a message or answer arriving as a turn ends is kept ([Concepts](docs/concepts.md#closing-a-session)).
- A fresh scheduled run someone wrote to stays open after answering them ([Always-on agents](docs/always-on-agents.md#what-it-is-made-of)).
- A repeated Stop within five seconds leaves the first stop to finish and keeps the reply's words. If the turn still has not stopped after five seconds, another Stop forces it to end and may lose words not yet recorded ([Concepts](docs/concepts.md#sessions-turns-and-states)).
- `razor devices add` waits up to five seconds for the relay to answer and warns alongside the code when this server is not connected, so you know another device may not reach it ([More machines](docs/machines.md#1-pair-two-servers)).
- A paired request whose connection must be re-established has five seconds to connect, so a dead relay path no longer holds it for thirty seconds ([More machines](docs/machines.md#what-pairing-gives-you)).
- A paired request that could not reach its server says nothing was sent; an uncertain result asks you to check before sending again ([Troubleshooting](docs/troubleshooting.md#more-machines)).
- A task or memory store clears its own Git index lock when a failed step finds it more than ten minutes old, then retries that step once; a younger lock's refusal gives the exact command to clear it after checking no Git process is using the store ([Git locks](docs/troubleshooting.md#git-locks)).
- Each task store has its own write lock, so a slow write to one project's tasks no longer blocks writes to another's ([Memory](docs/memory.md#tasks)).
- A home whose memory or tasks already have a different history keeps them local and gives a startup warning with migration steps, instead of failing to join a shared store ([Memory](docs/memory.md#several-machines)).
- Shared memory refuses folders outside its defined structure before refreshing or pushing, so a nested project's directory cannot be swept into the parent's next shared write. Nested projects under `state`, `wiki` or `residents` remain a limit; anything already pushed stays on the remote ([Memory](docs/memory.md#sharing-with-other-people)).
- A ruling written from your answer keeps both your words and the question you answered, so its source can be read later ([Memory](docs/memory.md#how-your-decisions-become-rulings)).
- Helper rows follow a continuation, stop or report on their owning server, including after a restart, instead of keeping an old state ([Concepts](docs/concepts.md#helpers-and-merging-or-discarding-their-work)).
- Discarding a helper refuses to remove detached commits that no branch keeps and tells the agent how to recover them; a clean worktree on another branch can be discarded ([Concepts](docs/concepts.md#helpers-and-merging-or-discarding-their-work)).
- Retrying a helper's merge still merges its branch if its worktree is already gone ([Concepts](docs/concepts.md#helpers-and-merging-or-discarding-their-work)).
- Discarding a paired helper whose record is gone clears the parent's remaining row ([Concepts](docs/concepts.md#helpers-and-merging-or-discarding-their-work)).
- An avatar install for a resident name that does not exist, including one containing a dot, refuses before writing a picture or settings ([Configuration](docs/configuration.md#the-terminal-app)).
- A failed helper Git command with no error text now says how it failed and asks you to inspect its folder before retrying ([Concepts](docs/concepts.md#helpers-and-merging-or-discarding-their-work)).
- Background release checks keep a held release's action notice through failed checks and clear it once that release is already installed ([Update Razor](docs/quickstart.md#update-razor)).

## 0.7.0 (13 November 2026)

### New

- In the terminal, `/search <words>` finds words anywhere in the conversation you are addressing, its tool calls and their results included, and Enter opens that place in the transcript ([Quickstart](docs/quickstart.md#6-quit-the-terminal-app-come-back-or-work-without-it)).
- The terminal reads what agents deliver: in the status pane's inbox, Enter opens a delivered file in the conversation, Markdown formatted and other text as written, up to 256 KiB, and `y` copies its path or link. HTML, PDF and other files still open in the phone or Mac app ([Deliveries](docs/configuration.md#deliveries)).
- In the terminal, the prefix and `h`/`l` or Tab/Shift-Tab move the keys among the agents list, the conversation and the status pane, as plain Tab and Shift-Tab do from the list or an open panel; a cyan edge marks the part that has them ([Moving between agents](docs/configuration.md#moving-between-agents)).
- **For notify and ask programs:** they get the sender's session id, name and project in `RAZOR_SESSION`, `RAZOR_NAME` and `RAZOR_PROJECT`, and a question's entry number in `RAZOR_ASK`, so a script can say who wrote and answer the question later ([Configuration](docs/configuration.md#notifications)).
- **For scripts and apps reading a session:** `GET /v1/sessions/<id>/search?q=<words>` searches that session's whole transcript, newest match first, at most 50 rows ([Using Razor from scripts](docs/scripting.md#the-local-api)).

### Changed

- Background updates keep your current version when a release needs your action; the terminal, `razor doctor` and a notification explain what to do before you run `razor update` and `razor restart`. Servers on 0.6.0 and earlier, or any server sharing their install folder, still select the next release automatically ([Update Razor](docs/quickstart.md#update-razor)).
- The guard that refused deleting files a session did not create is now off unless you turn it on with `razor config set guards.delete true`; installs that never set it take the new default. When on, it accepts `~/` paths and its refusals say what you can do ([Security](docs/security.md#what-agents-can-run)).
- `/status` is gone: Ctrl-T or the status chip opens the status pane and gives it the keys at the inbox, Escape gives them back, and `-` or a click on STATUS sets whether the pane stays open after the keys leave. Clicking a delivery opens it instead of copying its path.
- Releases ship agent-browser 0.43.0, its latest, in place of 0.42.0: a snapshot gives a hint (an icon, a test id, the row's text) for a control with no name or one it shares, large pages snapshot faster, and a command on a tab that closed fails at once instead of waiting out its timeout.
- Releases ship Cua Driver 0.49.1, Cua's newest, in place of 0.47.18: a refusal's text ends with what to try instead, reading a hidden Chrome or Electron window on a Mac shows the page as it is now, and on Linux it starts on Ubuntu 20.04 and captures a rootless Xwayland desktop. Its experimental picture-in-picture preview and the `escalate_session` and `get_session_state` tools are gone.
- **For scripts and apps reading the status stream:** the `agents` line adds `notices`, settings notices that stop nothing, so its `config` holds only errors that keep agents from starting; a `server` line may add `release_action`, saying what to do for a release that needs you ([Using Razor from scripts](docs/scripting.md#the-local-api)).
- **For scripts answering questions:** a question that cleared because you wrote to its session or closed it leaves its row's `asks`, and answering it is refused.
- **For scripts listing sessions:** `GET /v1/sessions` takes `skip_dreams=true` to leave out the nightly dream sessions, as `razor --continue` does; older servers ignore it.

### Fixed

- Failed shared task and memory writes are undone locally, so retrying a refused commit no longer duplicates comments or carries in a failed claim.
- Slow task and memory writes give git time to release its locks, so a timeout no longer blocks every later write ([Git locks](docs/troubleshooting.md#git-locks)).
- Retired settings and avatar pictures that could not load show as notices in the terminal's status pane, the server log and `razor doctor`, apart from errors that keep agents from starting. The phone and Mac apps, which do not show notices yet, no longer report them as such errors ([Configuration](docs/configuration.md#when-a-change-applies)).
- A question an agent asked you clears when you write to its session or close the session yourself; a session that closes on its own keeps its question, and MCP and sign-in questions still wait for an answer. Words handed over from a voice call clear it too and show as your message, without the iPhone's "You answered" row ([Concepts](docs/concepts.md#questions-notes-and-deliveries)).
- Stopping a GPT reply, or one from a model on a provider you configured, keeps the words it had written: they stay in the transcript and the record, and the agent sees them on its next turn.
- `razor --continue` no longer reopens the nightly dream session.
- A change to a resident's schedule or heartbeat, or adding a resident, takes effect within about a second instead of taking up to a minute.
- A finished helper whose report waited for its parent, or whose server restarted before the parent read it, keeps its branch, model and last activity in the agents list instead of showing them blank.
- Claude now receives an MCP tool's text alongside its structured data, so instructions in the text are no longer lost.

## 0.6.0 (12 November 2026)

### New

- Custom avatars. The starter skill `agent-avatar` helps an agent make one from a picture generated on your ChatGPT sign-in, check it and install it; `[brand.avatars]` names each picture, `/avatar <name>` gives one to a session, and an always-on agent's `avatar` setting gives one to it and its threads. The terminal's workshop scene draws them; the phone and Mac apps will in a later build and draw the usual creature until then ([Configuration](docs/configuration.md#the-terminal-app)).
- An always-on agent keeps its own memory beside its project's: its own state note, rulings, facts and pages, which only its sessions read ([Memory](docs/memory.md#an-always-on-agents-own-memory)).
- Shared memory works between people. Each change is committed as you, a ruling belongs to the person who added it, and only they retire or replace it; when people disagree, each one's agents follow their own person, and a new ruling citing you can settle the others' rulings, which their next session asks them about. When a repository you cloned already shares memory or tasks, a startup page asks once whether to join ([Memory](docs/memory.md#sharing-with-other-people)).
- A project whose memory is shared gets one nightly pass among your paired servers that can reach each other, which reads your sessions from all of them, instead of one pass on each; a server none of them reaches runs its own ([Memory](docs/memory.md#several-machines)).
- In a shared task list, a task can be addressed to one person with `for`: it shows as a question on their startup page alone, with the agent's reading of any term it was unsure of ([Memory](docs/memory.md#questions-for-another-person)).
- Every session starts with the project's glossary, the wiki page `glossary`, so agents use the project's terms as defined; dreaming keeps it ([Memory](docs/memory.md#the-glossary)).
- An agent can start a helper on a task. The helper claims it in its own project, on whichever server it runs; its report becomes a comment on the task when its parent settles it, and merging a helper that reported finished closes the task ([Memory](docs/memory.md#tasks)).
- Android notifications: a server given Firebase credentials sends the Android app's alerts and followed sessions through Firebase Cloud Messaging, ready for the app's update that reaches this release ([Configuration](docs/configuration.md#android-push-notifications)).
- `razor doctor` shows this server's network key, its relay connection and, for each paired server, whether it is reached directly or through a relay, with the round trip ([Troubleshooting](docs/troubleshooting.md#more-machines)).
- The starter pack ends every conversation's opening with `code-quality.md`, seven lines on writing simple, deep code.

### Changed

- Razor's machines and apps reach each other only by key, over connections encrypted end to end, and Razor no longer needs Tailscale or any VPN. The server no longer listens for plain HTTP at `[server] address`, and pairing by address, `razor devices find` and setup's question about other machines are gone ([More machines](docs/machines.md)).
- To update: run `razor update` and `razor restart` on every machine. If you were given a relay address, set it on each with `razor config set network.relays '["…"]'` before pairing anything ([Running your own relay](docs/relay.md)). Then pair every server and app again once by code, under the name `razor devices` already lists for it, so a phone keeps its notification settings: `razor devices add <name>` shows a code, then run `razor devices join <name>` on the other server and paste it, or scan it in the app's Settings ([More machines](docs/machines.md#servers-and-apps-paired-by-an-earlier-version)).
- A phone or Mac app from before this release cannot reach a server on this release and shows itself signed out until you update it and pair it again by code.
- A relay that admits only its own people takes one password per person, written into the relay address in that person's `[network] relays`; `razor doctor` and pairing's refusals show the address without it ([Running your own relay](docs/relay.md)).
- `[server] address` is no longer used: a config that sets it still loads, and `razor doctor` asks you to remove it. A `[remote]` that holds `address` and `key` no longer stops the server; starting a session says to name the paired server instead, as `server = "<name>"`.
- A settings error names the key and never repeats the value you gave, so a password or key typed in the wrong place stays out of `razor config`, `razor doctor`, the server log and the terminal's notices.
- The Android app does not reach a server on this release until its next update.
- Model names are families: when an update to Razor moves a family to a newer release, a session on `fable`, `opus` or `sol` uses it on its next turn, and a full id with a version stays where it is. Gemini, DeepSeek and GLM are families too (`gemini`, `deepseek`, `glm`), the picker lists families first, then the provider models you configured, and a session's status shows the model that actually answered beside its choice, when the provider reported one ([Models and sign-in](docs/models.md#family-names)).
- `fable` runs Fable 5.2, through Claude Agent SDK 0.3.321. Fast works on its full id, `claude-fable-5-2`, too, and the speed picker shows Fable 5.2's Fast price.
- A ruling is now one short plain rule, written by the agent, citing your message; Razor copies your exact words in beneath it. Dreaming may merge, refine or retire your rulings from those words, and asks you only when two of them truly contradict ([Memory](docs/memory.md#how-your-decisions-become-rulings)).
- Memory refuses text that looks like an API key or a private key, without repeating it, and dreaming retires an older memory holding one so you can rotate the key ([Memory](docs/memory.md#keys-and-secrets)).
- Dreaming skips folders that no longer exist and temporary or cache folders, and a dreaming session closes at the end of every turn, one you opened by answering it included ([Memory](docs/memory.md#dreaming)).
- When a session closes for good, Razor stops and discards the helpers it never settled, keeping each one's uncommitted work on its branch; a helper it cannot reach or preserve stays unsettled. Closing for being idle keeps them.
- A message you send while an agent works reaches it with a line asking for an answer before its next step, and an agent that was only waiting for a helper or a command is told to answer you and end its turn rather than wait again, so its answer reaches you instead of staying in its thinking.
- A session closing with `/done` while it works stays under Working in the agents list until it closes, instead of moving among the idle ones.
- A repository is named by its main checkout's folder, not by its origin's address, so its worktrees and a clone whose origin has another name keep the name you see on disk.
- `[sessions] handover_minutes` and `close_minutes` take fractions of a minute: `0.1` is six seconds ([Configuration](docs/configuration.md#models-and-sessions)).
- A stopping server tells the devices connected to it why their connection ended.
- Releases ship agent-browser 0.42.0, its latest, in place of 0.41.2: it keeps literal text exact when it fills or types, keeps fractional coordinates, reuses or relaunches a browser session more reliably, and tells agents to prefer snapshots to screenshots.
- Releases ship Cua Driver 0.47.18, Cua's newest, in place of 0.44.0: it refuses input to macOS security and consent prompts, waits for a control to become enabled after bringing its app forward, sends a middle click to the right window on macOS, and works with more Linux desktops (Hyprland, GNOME and KWin after an update).
- **For programs reaching the API from another machine:** the plain HTTP address and its device key are gone, so a dashboard or script that used `http://<address>` stops reaching Razor. A program on the same machine uses the local socket ([Scripting](docs/scripting.md#from-another-machine-or-program)); one elsewhere needs Razor's network library.
- **For scripts and apps reading devices and settings:** a device has no `address`; a paired server or app has `keys`, its network keys, and a server also its `server_id`, and no listing shows a device's relays. `/v1/pair` and `/v1/discover` are gone; `POST /v1/devices` takes `name` and an optional `key`, for a program on the local socket, and answers with the device alone, without `pairing`, `own`, `requested` or `answered`. The terminal offers a pairing code with `POST /v1/pairing`, joins with one through `POST /v1/pairing/join` and reads `GET /v1/network` for doctor; all three take only the local key on the socket. `server.json` holds only `pid`, and a session's settings answer leaves out `[network]`.
- **For scripts and apps reading the record:** `model` keeps the chosen family (`sol`, not `gpt-6.1-sol`) in a session's start, its inputs and each turn's end, and a turn's end adds `served_model` when the provider reported its id. A session's first entry may carry `project_root`, and a new `avatar` entry records a session's avatar.
- **For scripts and apps reading the agents feed:** a row has `closing_working`, true while a session asked to close as it worked, and may have `served_model` and `avatar`. The `agents` line's `brand` carries `resident_avatars`, and the pictures themselves with `?avatars=true` ([Scripting](docs/scripting.md#the-local-api)). `GET /v1/models` adds `labels`, each family's display name.
- **For scripts and apps reading the status stream and a session's events:** a `delta` carries `start` on the first words of each reply block, so a reader that joins mid-reply knows where the next reply begins.
- **For apps registering for notifications:** a registration names its `service`, `apns` unless it says `fcm`, and an FCM token has no `environment`.

### Fixed

- A resident lost its shell and the other tools a dream may not use after its first night of dreaming, and stayed that way through restarts and `razor --resume`: its own conversation was mistaken for the dream it had started. It keeps its tools now, and a resident already stuck gets them back on its next message after you update and restart Razor, with nothing to repair by hand.
- A phone paired again under a new name, with the same notification token, got each notification twice. A token now belongs to the last device that registered it.
- A session could stay working with nothing running, and never answer your next message, after a rebuild (at a handover, the context threshold, the night or a restart) while a call had details the agent had not yet been sent.
- A phone that opened a session while the agent was replying could drop that whole reply, and so could one watching when a Claude reply was cut off after its first words and tried again.
- Stopping the server could leave behind a Claude Code process that did not answer its close; the Claude bridge now waits for the SDK to end it. `razor stop`, `razor restart` and `razor install` still wait only a bounded time, now including up to three seconds to tell connected devices.
- With shared memory, two machines writing at the same moment could get "the remote did not answer" when the second should simply have tried again; it now does.

## 0.5.0 (8 November 2026)

### New

- After a rebuild or `/handover`, an agent gets back the skills it had loaded, read again from their files; a skill you handed it comes back from its recorded text if its file is gone. Any that do not fit are named so the agent can load them again, and the terminal's skill lamps mark the ones that wore off ([Concepts](docs/concepts.md#when-the-context-fills)).
- **For scripts and apps reading the agents feed and the record:** each agent on the agents feed has `skills`, the skills in its context (`name`, `entry`, `at`, `how`, and `worn` once it wore off), and a rebuild entry has `skills` (`name`, `how`, `outcome`) and `held`, the inputs waiting to be sent that it left out. Older apps ignore the new fields.

### Fixed

- On a busy night, dreaming could leave older sessions out of its input and never read them. It now lists every session since the last pass and includes each one's digest within its input limit, so it also recalls less.

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
