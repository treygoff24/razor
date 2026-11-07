# The flows

Razor's tests are the flows: numbered end-to-end scripts in `flows/`, and there are no others. Each one starts a built `razor` binary in a scratch home, uses it the way a person would, through the API, `--print` or the terminal app under a pseudo-terminal, and checks what a person would see (the reply, the file, the job's exit) and what the record holds.

Only the services outside Razor are stand-ins. The model stand-in is an HTTP server that replays recorded replies over OpenAI's Responses API, Anthropic's Messages API and OpenAI-compatible chat. On the Claude route the real Agent SDK runs its own Claude Code against it, pointed there by `ANTHROPIC_BASE_URL`, so the flow sees the exact request Claude would receive. Other flows stand in for MCP servers and their sign-ins, the web search providers, ElevenLabs, Apple's push service, live voice calls, a meeting recorder, Tailscale's command line, the `skills` installer and a release server.

Run them with `scripts/flows.sh` and flow numbers or names ([Contributing](contributing.md#tests)). Each file opens with a paragraph saying exactly what it checks, which is more than the one line each gets here.

## What they cover

**Sessions and the command line**

- 01 `razor doctor` in a fresh home: what is missing and the command that fixes it, no key printed, nothing written, exit 1 with nothing to run on and 0 with a sign-in; help, `--help` and refused commands change nothing; with a display, the server runs the shipped cua-driver as its daemon on the home's socket, with Cua's telemetry and update check off and no key of Razor's.
- 02 A reply through the real Claude SDK and `--print`, and what the startup list gives the model.
- 03 A first message with an image: stored, recorded, and sent to the model before the text.
- 04 Resume by id, continue the newest in a folder, and the three `--print` formats.
- 05 Escape stops a running command on both model routes; a client that disconnects leaves its session running.
- 36 (stop) In the terminal app, Ctrl-C detaches and Escape stops the command, not the app.

**Shell, jobs and files**

- 06 Shell results, and a correction from the person before the agent's next step.
- 07 Background jobs: waiting on them, looking at them, stopping them, and a job's end waking the session.
- 08 An interactive program under a real terminal; a secret typed from a file never enters the record.
- 09 Independent reads in one model message run at once; two edits to one file apply in order.
- 10 Reading by line, an edit that must match exactly once, and writing into a new folder.
- 38 The two command guards refuse before anything runs; Razor's own keys never reach agents' shells.
- 39 Script tools on both model routes.

**Helpers**

- 11 A shared brief reaches a fresh helper; its branch merges and its worktree goes, its uncommitted files kept.
- 12 A reviewer's branch is discarded: the worktree goes, the committed branch stays.
- 13 A helper's question reaches its parent and the answer comes back; outside git a helper works in its parent's folder.
- 14 The twenty-first working helper is refused, and raising the limit live lets it start.
- 15 Only a mention in the channel wakes an agent; the status lists headlines and files.
- 16 A finished helper continues on its own branch; a helper given no model runs on its parent's.
- 36 (steering) Typing into a helper's card in the terminal app reaches that helper and no other.
- 40 A helper that stops without reporting: the grace period, the reminder, and its jobs.
- 49 A helper on a paired server: mail, waiting, continuing, stopping, settling and a restart.

**Mail between agents**

- 42 Mail within a tree arrives under a header naming its sender.
- 43 Mail between top-level sessions on one server, by name or project.
- 44 Mail between two paired servers.
- 45 Mail through [Post](https://github.com/treygoff24/post), when it is installed.

**When the context fills, restarts and closing**

- 17 A rebuild at the threshold keeps your requests word for word, and `recall` returns an entry from before it.
- 18 `/handover` runs the same rebuild on demand.
- 19 The server is killed mid-work: the session, its helper and its running job all come back.
- 20 `/done` answers at once, its closing turn writes the state note, and the session closes.
- 34 Messages sent during a rebuild reach the fresh context once each, in order.
- 35 A crash between a command's work and its recorded result: the outcome is unknown and the command never runs again.
- 66 A session waiting past `[stuck] after` with unanswered input shows as stuck.
- 67 A message sent while the model is replying ends an `agent wait` and reaches the next request.

**Model routes and their failures**

- 28 GPT's code mode: Codex's tools, its web search, images and image generation.
- 31 A GPT stream that ends early or drops a call runs no tool.
- 32 A tool call with malformed arguments gets an error and changes nothing.
- 33 A reply split at any byte, inside a character or an escape, parses the same.
- 41 Claude sign-ins stay with each conversation across resumes and restarts; account usage reaches the status pane.
- 46 Chat models: the wire, the plain tools, and each host's way of carrying reasoning back.
- 65 Thinking summaries from both Claude and GPT in the terminal app.
- 73 Thoughts stream as live lines on every route, numbered through the session and never recorded or replayed; a redacted thought sends nothing, and a cut response's thoughts are voided before its retry.
- 68 Switching models within and across families, and what each switch costs.

**Always-on agents**

- 21 The background service and its wrapper; a resident starts unprompted, skips a heartbeat while busy and holds a schedule for the next turn.
- 22 A schedule runs on its own model and effort and ends with a note or a question; fresh runs; a resident's projects.
- 23 A resident's question answered through the `ask` program or the API, once.
- 64 A resident's threads in the agents list, before and after a restart.

**Memory and tasks**

- 24 Memory files: whole-word search, retiring, the state note and rulings in a new session, a helper sharing its parent's memory, and tasks shared between two homes through a git remote.
- 79 Reminders to bring tasks and notes up to date: one after two helpers report, the count starting again after a task write, and the same upkeep asked for before an idle handover's continuation note.

**The API and the terminal app**

- 25 The API: a device key, sessions, pages, attaching, sending and revoking.
- 75 Pairing: `razor devices find` discovers real Razor servers through a stand-in for Tailscale's command line, both machines confirm without carrying a key, forged requests are refused, `razor doctor` sends no credential anywhere, the status stream opens with its `agents` line while a paired agent keeps changing, and a late network bind recovers.
- 26 The terminal app: the transcript, a helper's card, the question band, the status pane and MCP.
- 80 Saved transcript images in the terminal: Kitty protocol detection and transfers, coloured half-block fallback, small viewport chips and clearing after scroll.
- 82 Large images pasted into the terminal app: a big PNG, a 12-megapixel photo and a JPEG are fitted to the model's limit before the request, the file on disk unchanged; one too large to read goes as its path alone, and one that cannot be fitted as itself; an image's original path reaches the model, after a rebuild too, without entering your words.
- 47 Questions in the terminal app: answering by number or in words, and two at once.
- 56 Saved views of the agents list, shared between two servers and edited through `$VISUAL` or `$EDITOR`.
- 58 An older terminal app meeting a newer server's line says so once and shows the rest.
- 70 Catch-up summaries: leaving a session records the last entry you saw, and ten minutes after your last look, not nine, the terminal app summarises what came after it, through the session's own sign-in or provider, the agent's words ahead of its newest steps.
- 74 The agents overview and list: the overview fits its agents, opens the selected one and folds at once without motion or below 80 columns; bound keys move between the list and a composer, past helpers; and a conversation's unsent words survive every move, whether its draft loads late, a read or save fails, eight writes wait, the words pass 1 MiB, or `--resume` brings a first message.
- 77 Suggested next messages ride in the main request: turning them on or off changes only the newest input, an idle handover keeps the offer, later work, a failed turn or an open question withholds it, and a malformed or disguised suggestion line stays ordinary text.
- 78 The terminal app shows the server's suggestion in the composer's grey hint; Tab puts it in the draft for editing, sending is separate, and Esc dismisses it once the status pane is closed.
- 83 Without running Razor: the colours DESIGN.md names are exactly the terminal's theme, name for name and colour for colour.
- 69 The session list over HTTP: roots newest first, narrowed to a folder, with the limit counted after narrowing.

**Settings and extensions**

- 27 Plugins: a JavaScript plugin's command and tool, and a hook that refuses a call with its sentence.
- 29 The web tools fall through to the next provider; Claude's own web tools stay.
- 30 A pack's resident, skill and brand, laid under your own settings key by key.
- 37 MCP servers: project files, live changes, sign-ins, search and questions; the command line lists and previews in plain text.
- 76 Skill updates through real Git and a stand-in `skills` installer, under one lock every Razor home shares: the pack stays untouched, a moving starter skill is published through an override, opting out is live, and an update that hangs is stopped without stalling the server.

**Voice, video and phone clients**

- 48 Push alerts to a phone through Apple's service.
- 50 Voice notes: transcribed before admission, kept with their words, refused with a sentence when the transcriber fails.
- 51, 52, 53 Recording and playing in the terminal app, over SSH, and on quitting mid-recording.
- 54 Video: a Gemini session watches it; any other session gets a watcher helper.
- 55 The bundled ElevenLabs speech program.
- 57 Agents' spoken notes, their limits and their settings.
- 71 Meetings recorded from a phone, against a stand-in recorder: live audio kept in order and synced to disk, speakers named, the live transcript shown without starting a turn, missing pieces found, the finished transcript as one note to the session, retry and cancel, and a helper's meeting outliving its worktree.
- 62, 63 Live voice calls and how they end.

**Installing**

- 59 `razor setup` and `razor keys` in a real terminal: hidden keys saved privately and kept on Enter, running setup twice, and `--console` reaching Claude Code's own sign-in.
- 60 The installer: a bad checksum keeps the current version; old versions are pruned, the current and previous kept; the first install enters setup in a terminal, an update prints the restart line.
- 81 Release updates: a local HTTP release installs with its checksum and npm dependencies while the old server keeps running; readiness appears once in a newly launched terminal, restart selects the new version, and opt-out, source builds, cached installs and failed downloads keep their promised behavior.
- 72 `razor stop` and `razor restart`: the lock an agent's file read cannot free, a lock with no PID refused, a second home beside another home's service, and install removing the service file it wrote when no manager answers.

## The evaluation

Razor is also measured against real models, on fifteen bounded tasks per model family with a finish check a script can run, before each release and whenever a model profile changes. The tasks and their runner are kept out of this repository so that the models being measured never see them.
