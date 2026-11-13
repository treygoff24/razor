# Concepts

Razor uses a small, fixed vocabulary: the same words appear in its settings, its messages and these pages. The sections below explain the ones you meet when using it, and the glossary at the end defines each in a line.

## The server and its clients

The server does all the work, and everything else, the terminal app included, is a client of it. The server talks to the models, runs the agents' commands, and writes every record. The terminal app, `razor --print`, the `razor mcp`, `razor login` and `razor devices` commands, and the Razor servers and apps on your other machines and devices that you pair with it ([More machines](machines.md)) all reach it through its API: on this machine through a socket in its home folder, with a key on every request, and from other machines over connections encrypted end to end, which only a paired device's key can open. Your own programs can use it too ([Using Razor from scripts](scripting.md)).

One server runs for each Razor home, the folder that holds its settings, keys and records: `~/.razor`, or wherever `RAZOR_HOME` points. `razor install` registers the server as a service, a background program that systemd (on Linux) or launchd (on macOS) starts when you log in. When no server is running, the terminal app and the other client commands start one: the service if it is installed, otherwise a server in the background. Either keeps running after the app quits; `razor stop` stops it and `razor restart` starts it afresh.

## Sessions, turns and states

| Word | What it means |
| --- | --- |
| session | One conversation with one model. It has an id (such as `20261101-203800-b659`), a name, the folder it started in, a model, and an effort (how much work the model puts into each step, `low` to `max`); `/model` and `/effort` change them. |
| turn | The session's work from one input until it waits again: reading, running commands, editing, replying. |
| input | Anything that starts or steers a turn: your message, a schedule firing, or mail from another agent. A message you send while the agent works reaches it before its next step. |
| state | One of four per agent: *needs you* (it asked you something), *working*, *done*, or *stuck* (unanswered input for a while with nothing running). |

Escape stops the current turn and the commands it started. Repeating Stop within five seconds leaves that first stop to finish and preserves the reply's words. If the turn still has not stopped after five seconds, another Stop forces it to end and may lose words not yet recorded. Ctrl-C twice quits the terminal app and leaves the session running.

When your message reaches a working agent, it must answer in text before more calls. If it tries calls first, Razor withholds one step and asks it to answer; those attempts stay in the record and leave no failed call cards. Calls already running finish, including calls from a running script, and a direct handover can proceed.

`/name <new name>` renames the addressed session immediately: its top bar and agents list row update together, including on a paired server. Names are one line of 1 to 64 characters; a helper's name is its parent's to give.

`/avatar <name>` selects an avatar; see [configuration](configuration.md#the-terminal-app) for the selection and clearing rules.

## The record

Each session's record is an append-only log, `~/.razor/sessions/<id>/record.jsonl`. Every message, tool call and result is an entry with an id, and an agent can recall any entry whole by its id. The record is the truth about a session: after a restart the server rebuilds everything else from it, and a call whose result was never recorded is marked as an unknown outcome, never run a second time.

Your past conversations can be searched by words, by any agent with its `recall` tool or by a program through the API (`GET /v1/search?q=`): every session you started here and on each paired server, by what you wrote, what the agent replied, mail and helpers' reports, and the session's name. Beside its record, each session keeps a small file of the text that is searched, which Razor rebuilds from the record if it is deleted.

The terminal's `/search <words>` searches the whole recorded transcript of the session you are addressing, including its tool calls, results and displayed Thinking summaries. It opens the chosen transcript row. It reads only that session, including when you address a helper; it does not search other sessions or attached files. Tool arguments are searched in full, even where their card shows only a preview.

## When the context fills

When a session's context, what its model can hold at once, fills up, Razor starts a fresh one built from the record, with no summary written by a model. At the threshold (by default 80% of the context, or 450,000 tokens on a model that holds 500,000 or more), Razor asks the agent for a continuation note: a few lines on the work in hand and the decisions behind it. The fresh context then holds:

- your latest requests, word for word;
- the continuation note;
- one line for each tool call since your last request;
- the files changed in the session's folder;
- the commands still running;
- its helpers and their states;
- the skills it had loaded, read again from their files (a skill you handed it comes back from its recorded text if its file is gone); any that do not fit are named so the agent can load them again.

Everything earlier stays in the record, one recall away. Razor calls this a rebuild, and the same one happens when you type `/handover`, after a session holding at least 50,000 tokens has sat idle for 50 minutes (`[sessions]` settings), when you switch to a model that cannot continue the conversation, after a restart that the model's own conversation cannot survive, and each night for an always-on agent.

## Closing a session

`/done` asks the agent for one last turn: it records what it learned, closes finished tasks and writes its own state note, a short note on where the work stands. Later sessions start with the project's merged note and newer sessions' notes; [dreaming](memory.md#dreaming) merges them. An always-on agent's home session writes the state note of its own layer, and a closing thread sends it what to keep; other sessions in its folder write project state notes. The session closes when that turn ends. `/end` (or `/quit`) closes it at once, with no last turn. `/detach` (or `/background` or `/exit`) leaves the terminal app with the session still running, as Ctrl-C pressed twice does unless [`[ui] quit`](configuration.md#the-terminal-app) says to end it. A closed session reopens when you send it a message.

An automatic close waits until all inputs have been answered. A fresh scheduled run you wrote to stays open after answering you; a dreaming session closes after the last input is answered.

`/done` and `/quit` clear the command from the composer as soon as you send it. If you type more before closing finishes, the terminal stays on the closed session with those words; sending them reopens it.

## Helpers, and merging or discarding their work

An agent can hand a task to a helper: another agent with a name, a brief (the task written out for it, with pointers), and a fresh context. Razor's screens, settings and branch names call a helper a **child**.

| Question | Answer |
| --- | --- |
| Where it works | In a git repository, its own worktree: a separate checkout at `~/.razor/sessions/<id>/worktree` on its own branch, `child/<id>`, started from its parent's current commit. It can instead be told to work straight in a folder, and one started outside git works in its parent's folder. |
| How it ends | It works, commits, and reports to its parent. |
| How many | A child can start children of its own. A session may have 20 working children at once by default, working meaning still running, not yet reported or stopped; `[children] max` changes it, live. |
| What it starts with | Its brief, the project's `AGENTS.md` and `CLAUDE.md` files (`[children] startup`), and nothing of its parent's conversation. |

You ask for a helper in plain words, such as "start a helper to add tests for the parser", or an agent decides to use one. The agent starting it gives it a name and a brief, and may also choose:

- a model, effort and speed of its own; without them the helper gets its parent's model and effort, at standard speed;
- a role, reviewer or researcher, which adds Razor's instructions for that role;
- a task of the project's to claim, so the helper's report lands on that task;
- a shared brief: a file whose text goes ahead of several helpers' own briefs;
- a folder, and whether to work in a worktree or straight in that folder; a worktree starts from the last commit, so work the helper must see is committed first;
- a paired machine to run on ([More machines](machines.md)); the helper then works in that machine's checkout, and its branch is merged there.

While a helper works, its parent can send it mail and it can ask its parent questions. A helper that stops without reporting gets 3 minutes (`[children] grace`), then one reminder to finish. After it reports, its parent can give it more work on the same branch before settling.

When a helper reports, the agent that started it, its parent, decides what happens to the work; you can tell it what you want done. Razor calls this decision settling, and it has three outcomes:

- Merge: the branch is merged into the parent's checkout, and the helper's worktree and branch are removed. A merge that conflicts is undone, and git's words go back to the parent.
- Discard: uncommitted files are committed on the helper's branch before the worktree is removed; the branch stays in git, unmerged, so you can still merge or delete it yourself. Detached commits must have a branch keeping them. A clean worktree on another branch can be removed; uncommitted files on another branch must be committed there or brought back to the helper's branch first.
- Merged by hand: Razor records that someone already merged the branch, then removes the worktree, and the branch once git agrees it is merged.

On a merge or a settlement recorded as already merged, files the helper left uncommitted are moved to `~/.razor/sessions/<id>/uncommitted`. A refused Git operation leaves the helper unsettled for recovery. If a merge is retried after its worktree is gone, Razor still merges the surviving branch. Discarding a paired helper whose record is already gone clears its parent's remaining helper fact.

## Agents talking to each other

An agent, its children and theirs share one channel: a group conversation in which a message reaches only the agents it names, and an agent that was waiting starts work when it is named. Any agent can also send mail to any other, by name or session id, on this server or one paired with it: a note, a question or an answer. Mail carries information, never orders, and it says who sent it.

## Questions, notes and deliveries

An agent can ask you a question that waits for your answer; it appears in a band under the conversation, and the number keys answer it. Questions an agent asks you clear when you write to its session or close it yourself; MCP and sign-in questions still need an answer. A session that closes on its own keeps its question. A late answer to an agent's settled question still reaches the session and reopens it if closed. Agents also deliver things to your inbox: a file, a link, or a note. In the terminal app, Ctrl-T opens the inbox in the status pane, and Enter there reads a delivered file in the conversation ([Deliveries](configuration.md#deliveries)). Questions and notes from the sessions you started yourself, as opposed to their children, also go to the programs named in the `ask` and `notify` settings. The programs receive the sender's session id, name and project, and a question's entry number, as environment variables. In a new install those show desktop notifications; they can reach your phone instead ([Configuration](configuration.md#notifications)).

## Always-on agents

An always-on agent is an agent you define in your settings that runs as part of the server, in its own folder, and wakes at the times you set. Razor's settings call it a **resident**. [Always-on agents and schedules](always-on-agents.md) explains how to set one up.

## Memory

Each project keeps a memory across sessions. A session starts with its state notes, your rulings (each a short rule that keeps your own words), the project's glossary, a list of wiki pages and recent facts and notes, and its tasks. A nightly dreaming pass checks memory against its sources and recent sessions, files lasting knowledge in wiki pages and merges state notes. [Memory](memory.md) explains what is kept and where, and how to run dreaming or turn it off.

## Settings and packs

Settings are TOML files that Razor rereads when you save them. Your own file is `~/.razor/config.toml`; a project can add `.razor/config.toml` (shared) and `.razor/config.local.toml` (private); and a pack sits beneath them all: a folder of default settings, prompt files and skills. Razor ships one, the starter pack, which a new install uses. See [Configuration](configuration.md).

## Tools

Every agent has Razor's own tools, whatever its model:

| Tool | What it does |
| --- | --- |
| files | Reading, writing and editing files. |
| shell | Commands in bash, in the session's folder. A long command becomes a job the agent can wait on, peek at, type into or kill, and an interactive program can run under a real terminal of 40 rows by 120 columns. |
| `skill` | Loads a skill by name: a folder of written instructions for one kind of task. |
| `agent` | Starts, steers, waits for, stops and settles helpers; sets the session's headline and name. |
| `message` | Mail to any agent, posts to the channel, and notes and questions to you. |
| `status` | The agents in its tree and what each is doing. |
| `recall` | Any entry of the record, whole, by its id; a search of your past conversations by words; or, with only `session`, its digest. Add `@<server>` to a session id on a paired server. |
| `memory` | Searches, reads and writes the project's memory ([Memory](memory.md)). |
| `task` | The project's tasks: lists, adds, claims, comments on and closes them. |
| `deliver` | Hands you a file or a link with a line saying what it is. |
| `handover` | Writes a continuation note and rebuilds the context on purpose. |

The file and shell tools take the form each model family knows best. Claude gets Claude Code's own `Read`, `Edit` and `Write`, Razor's shell in place of Claude Code's, and Claude's own web search and fetch. GPT on the ChatGPT sign-in runs in code mode by default: an `exec` tool that runs JavaScript calling the tools Codex shows it, such as `exec_command` and `apply_patch`, and `wait` for a run still going ([Models and sign-in](models.md#gpt)). Every other model, and GPT without code mode, gets plain tools: `read`, `write`, `edit` and `bash`. Agents' shells also have ripgrep (`rg`) and `jq`, which ship with Razor and come last on their `PATH`, so copies you already have are the ones they use; your own shell's `PATH` is unchanged.

More tools appear as you set them up:

- `search`, `fetch` and `answer` for the web, while a provider key is set ([Browser, voice and search](browser-voice-search.md#web-search-page-reading-and-answers));
- a real browser, through the `agent-browser` command in the shell ([Browser, voice and search](browser-voice-search.md#the-browser));
- desktop apps, through the `cua-driver` command, on a machine with a desktop that allows it ([Browser, voice and search](browser-voice-search.md#desktop-apps));
- MCP servers' tools; when a server has many, agents find them with `mcp_search` instead of seeing them all at the start ([Configuration](configuration.md#mcp-servers));
- your own scripts as tools, and tools that JavaScript plugins add ([Configuration](configuration.md#script-tools)).

## Glossary

The words you meet in Razor's screens, settings and these pages. Words used only inside the code are in [Contributing](contributing.md#words-used-in-the-code).

- **account**: the Claude or ChatGPT subscription a conversation signs in to; the usage panel shows that account's plan usage.
- **agents view**: the terminal app's list of agents beside the open session, opened with F2, `/agents` or Left at the composer, and its full-screen overview of every agent, opened with a second Left or F3.
- **ask**: a question from an agent that waits for your answer.
- **brief**: what a child starts from: its task and pointers. A **shared brief** is a file whose text goes ahead of several children's own briefs.
- **channel**: the one conversation shared by a session and all the children under it. A **mention** of an agent's name is what wakes it there.
- **child**: an agent a session starts for one task, called a helper in these pages, with a name, a brief, a fresh context and its own worktree; it reports and leaves a branch its parent merges or discards.
- **claim**: a session's hold on a task, so other agents leave it be; the session's server releases it when the session closes, or when its parent settles a child started on it.
- **closeout**: a session's last turn, asked for with `/done`: it brings the project's notes up to date and writes the state note, and the session closes when it ends.
- **context**: what a model can see at once; it fills as a session goes on, and a rebuild starts a fresh one.
- **continuation note**: the few lines an agent writes for a rebuild, on the work in hand and the decisions behind it.
- **delivery**: a file, link or note an agent hands to your inbox.
- **effort**: how much work a model puts into each step: `low`, `medium`, `high`, `xhigh` or `max`.
- **fresh run**: a schedule's run in a thread of its own, which closes when the run ends.
- **grace**: how long Razor waits after a child stops without reporting (`[children] grace`, 3 minutes by default). Then it sends one **nudge**, an input asking the child to finish.
- **group**: a named, coloured part of a view, with matches and up to three levels total.
- **headline**: the one line saying what an agent is working on now.
- **heartbeat**: an input that wakes a resident at set times.
- **home session**: a resident's own session, whose id is its name; its heartbeat, night and schedules run there.
- **input**: anything that enters a session: your message, a schedule firing, mail from another agent. Every input is admitted, recorded and queued, the same way.
- **job**: a command the shell started. One that outlives its call has a **handle** the agent uses to wait on it, peek at it, type into it or kill it.
- **mail**: a note, question or answer from one agent to another. It carries information, never orders.
- **match**: a repository, folder or session that places a top-level session in a group of a view. A repository is named by its main checkout's folder, so its worktrees carry the same name.
- **memory**: what a project keeps across sessions: Razor's own memory files in its home's `memory` folder, and its tasks. The **startup page** is what a session reads from it at its start: the project's **state note** (the merged note and newer sessions' handoffs), its **rulings** (what you decided, each a short rule citing your message, with your words kept beneath it; only you can make one), its wiki contents and recent **facts** and **notes**, and the session's tasks. An always-on agent also keeps its own **layer** of memory inside its project's, which only its own sessions read.
- **pack**: a folder of default settings, prompt files and skills that sits beneath your own settings; Razor ships one, the starter pack, which a new install uses.
- **pair**: to connect two machines' Razor servers, each holding a key for the other, so each shows and reaches the other's sessions (`razor devices add` on one shows a code; `razor devices join` on the other takes it).
- **plugin**: a program or JavaScript file named in the settings that sees **events** (a prompt, an input, a tool call, a rebuild) and may observe, change or refuse them. A **hook** is a plugin written as a program.
- **provider**: where a group of models is reached on an API key: the API it speaks, its address and the variable holding its key, kept in `keys.toml`; a session names its models `<provider>/<id>` ([Models and sign-in](models.md#providers)).
- **rebuild**: making a fresh context from the record (your latest requests word for word, the continuation note, what happened since) when the context fills, on `/handover`, after a restart, or at a resident's night.
- **record**: a session's append-only log on disk. Each **entry** in it has an id and can be recalled whole.
- **resident**: an agent that runs as part of the server, called an always-on agent in these pages, with a persona, memory, a schedule and an identity record; it has a home session and threads.
- **resident project**: a folder in a resident's `projects` folder, shown by the name in its `.razor/name`; threads can start in one, and renaming it never moves the folder.
- **run**: one heartbeat, schedule or night of a resident, followed until the turn that answers it ends.
- **service**: the background program `razor install` registers with systemd or launchd: the same server `razor serve` runs, started when you log in.
- **session**: one conversation with one model, with its own record; it has an id and can be reopened. A **top-level** session is one a person started, or one started for the person by a top-level agent; it is not a child.
- **settle**: the parent's decision on a finished or stopped child's work: merge it, discard it, or record that it was merged by hand. When a parent closes, Razor discards its unsettled helpers, preserving their uncommitted work on their branches before removing their worktrees.
- **skill**: a folder of written instructions, a `SKILL.md` and anything beside it, that an agent loads by name.
- **state**: one of four per agent: needs you, working, done, stuck.
- **task**: a piece of work a project keeps across sessions, with a number, a title and a description, open until it is closed with a reason; it may be blocked by other tasks, belong to a parent task, be claimed by one session and be due by a date. Anything owed, an agent's promises included, is a task with a due date.
- **thread**: a fresh session started as a resident, with its identity record, folder, startup files and memory.
- **turn**: a session's work from one input until it waits again.
- **view**: a named, saved arrangement of the agents list, edited with `/views`.
- **worktree**: a separate git checkout on a child's own branch. A child's **placement** says whether it works in a worktree or straight in a folder it was given; one started outside git works in its parent's folder.
