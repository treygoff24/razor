# Memory

Each project has a memory that outlasts its sessions, so a new session starts knowing where the work stands and what you decided. Razor keeps it in plain Markdown files in its home, one file per memory, which you can read, edit and delete like any other file. It also keeps the project's tasks, the work still to do (see [Tasks](#tasks)).

## What a session starts with

Every session opens with its project's startup page, at most about 2,500 tokens. It holds the project's merged state note, then the newer notes sessions wrote when they closed with `/done`, newest first: the current work and the decisions behind it, together in at most 1,500 bytes. Each session keeps its own note, so parallel sessions preserve each other's handoffs. Session notes expire after 14 days unless given another date. Below it come your standing rulings: short plain rules, at most twenty lines in 4,000 bytes, newest first, each naming its owner in a [shared project](#sharing-with-other-people). Read a ruling whole to see your original words and citations. Then the project's [glossary](#the-glossary), the wiki pages' names and descriptions, and the facts and notes written since the last dream, newest first, at most 24 lines together and then a count of the rest; and last, the tasks the session holds, the open tasks due within a week, and how many are ready.

Nothing else is added to a session's context by itself. During the work, agents search memory, read a memory whole, and write new ones with their `memory` tool, whenever you say "remember this" or they find something worth keeping.

## What is kept

| Kind | What it is |
| --- | --- |
| state note | A session's current work and decisions, merged into the project's state by dreaming |
| ruling | A standing decision governing future work, written as one short plain rule citing your message |
| fact | Something true about the project or the world that later sessions should know, with a one-line description |
| note | Anything else worth keeping, with a one-line description |
| wiki page | Lasting knowledge filed by dreaming, with sources and a description saying when to read it |

A memory belongs to its project, or to every project when the agent writes it that way. Facts and notes can carry an expiry date, after which they move into the retired folder. Rulings have no expiry; a one-time approval belongs in the approved task's comments, a bug report becomes a task, and a one-session instruction stays in that session. A fact that changes is written again, replacing the old one.

## How your decisions become rulings

A ruling can only come from you. The agent writes one short plain rule and cites the message of yours, or your answer to a question, that it comes from. Razor checks that the cited entry holds your words and copies them into the ruling file under the rule. An answer also keeps a citation to the question it answered. It never attaches the agent's previous reply as a question. Several rulings may cite the same message.

For example, when you decide that every merge waits for the tests, the startup page shows that rule. Reading it whole shows your original message, even if the session has since been deleted or the memory has reached another machine. Older ruling files show their agent's-reading line until dreaming rewrites them.

A rewrite keeps all original citations and quotations. An agent can merge several rulings by replacing their ids together; the originals move to the retired tree. Dreaming checks the rules against those original words, and may merge duplicates, refine a rule or retire an outdated one. It asks you only when two of your rulings truly contradict each other.

Scope follows the subject of a ruling. The memory and task tools accept a `project` folder, absolute or relative to the session, to read or write another project's memory or tasks. Without it they use the session's project; `all` means every project and cannot go with `project`. A missing folder or a file is refused. An existing folder outside git is its own project, just as for a session.

A helper (a child) writes no memory at all: it tells its parent what is worth keeping.

## An always-on agent's own memory

An always-on agent working in a project keeps its own memory beside the project's: its own state note, rulings, facts, notes and pages, in `residents/<name>/` inside the project's memory folder. Its home session and threads start with the project's page and then their own, under headings with the agent's name; any other session in the folder sees the project's page alone. The agent reads and writes its own memory with the memory tool's `resident: true`, which only its own sessions may use, whatever folder a session is in. Its home session's state note goes there by itself; a thread sends the home session a note of what to keep; other sessions in the folder write the project's state notes. A task can be tagged with the agent's name: it stays in the project's list but off other sessions' pages.

What goes where follows one question: would another agent working in this repository do better knowing it? Then it belongs to the project. Only knowledge about how the agent does its own job belongs in its own memory, and instructions meant only for the agent belong in its startup files. The project's nightly dream runs as the agent and tidies both, moving what was filed in the wrong place.

## Keys and secrets

A memory write is refused, before any file changes, when its text holds what looks like an API key (a word starting with a known key prefix, such as `sk-`, `ghp_` or `AKIA`, and at least 16 more key characters) or a PEM private-key block, a ruling's quoted words included. The refusal never repeats the key. Keep only where the secret is stored. Dreaming retires an older memory holding a key and reports it so the key can be rotated.

## Where the files are

Memories for one project are in `~/.razor/memory/projects/`, followed by the project folder's own path; memories for every project are in `~/.razor/memory/all/`:

```text
~/.razor/memory/
  all/
    fact-20261101-203800-0a1b2c3d.md
  projects/home/you/code/app/
    state.md
    state/20261102-081500-77aa.md
    ruling-20261101-204512-9f3e11c0.md
    note-20261102-081500-77aa0b12.md
```

The project is the repository's main checkout, so a helper working in its own worktree shares its parent's memory; outside git, it is the folder the session started in. A session records its project root when it starts, so dreaming can still find its project after a worktree is removed. Older sessions resolve their folder when discovery runs. Starting a session in a missing folder is refused. A memory's file name says its kind and when it was written, and its id is the name without `.md`. Each file starts with a short header (where the memory came from, any expiry date, and a fact's or note's description) between `---` lines, then the text:

```text
---
source: razor:20261101-203800-b659:l-12
---
the agent asked: "Should every merge wait for the tests?"

the human said (razor:20261101-203800-b659:l-12): "yes"

agent's reading: merges wait for the tests.
```

You can edit a file, delete it, or add your own; any other `.md` file is a note, and the next memory call sees the change. A link counts as the file or folder it points to, so a project's memory folder can be a link to a folder you keep elsewhere, such as in a repository of your own. Each session replaces only its own `state/<session>.md`; dreaming merges the notes into `state.md`. If you move a project to another folder, move its memory folder and its [tasks folder](#tasks) to match. With `RAZOR_HOME` set, the memory folder is `$RAZOR_HOME/memory`.

## Retired and deleted memories

A retired memory is one no longer in use but kept on disk: when an agent replaces or retires a memory, Razor moves its file to the same place under `~/.razor/memory/retired/`, such as `retired/projects/home/you/code/app/ruling-20261101-204512-9f3e11c0.md`. Search, reading a memory and the startup page never look in that folder.

- Retired: out of use, still on disk. To bring one back, move its file back to the same place outside `retired/`.
- Deleted: gone for good, whether you delete the file yourself or, for an always-on agent's memory, a client deletes it through the server's API.

## How agents search memory

Search matches the query's words as whole words, ignoring case, in a memory's text (not its header, so not a fact's description). Results are ranked by how many of the query's words each memory holds; among memories that hold the same number, the newest comes first. Newest here and on the startup page goes by the time in a ruling's, fact's or note's file name, so a memory fetched from another machine keeps its place; a file with no time in its name goes by when it last changed on disk. A search returns 8 results unless the agent asks for up to 20. Search reads the files each time and holds nothing in memory between calls. It was measured with 2,000 memories in a project and 2,000 for every project; past that, each search takes longer, because it reads every file, but there is no limit.

## Wiki pages

Dreaming keeps durable knowledge in `wiki/` under the project's memory folder, or `all/wiki/` for knowledge shared by every project. Each page has a `name`, a one-line `description` saying when to read it, and a `checked` date. The startup page lists these headers first, followed by facts and notes written since the last dream. Search and get include pages, whose ids are `wiki/<name>`.

Only dreaming writes pages through the memory tool, at most 200 pages in each wiki and 16 KiB per page. Pages cite the original sources and link with `[[name]]`. A filed fact is retired after its knowledge reaches a page. Conflicting claims remain visible with their sources; replaced values keep a dated history.

### The glossary

The wiki page `glossary` holds the terms a project's memory uses that a new reader would not know, one to a line, as `- term: meaning`. Every session starts with it: up to twelve lines in 1,200 bytes, shown above the list of pages and taken out of that list's 24 lines and 2,400 bytes, so the page grows no longer; a longer glossary says how many more lines `memory get` reads, and an empty one takes no room. Dreaming keeps it. An agent that coins a term uses plain words instead, or defines the term in a note (`term: meaning`), which is on every startup page at once and which dreaming moves into the glossary; dreaming rewords a memory that coins a term it does not need, and adds a line for a term inside a ruling, which it cannot reword.

You can write `wiki/_instructions.md` to say what matters for a project. Dreaming reads it first and cannot change it. The file stays yours to edit.

## Sources

Facts and notes can name their sources with `cites`: session entries as `razor:<session>:l-<entry>`, code or design as `file:<path>@<commit>`, cached references as `ref:<path>`, fetched pages as `url:<url> <date>`, and obligations as `task:<number>`. Each citation is a header line beside `source`, which still names the entry that wrote the memory. Rulings continue to quote your words from the record.

For the nightly pass, Razor checks code citations against main and live branches. They are on main, changed since the cited commit, awaiting merge on a branch, or discarded. Discarding a helper counts even while its branch remains on disk. A missing cached reference is listed too.

## Dreaming

At 03:00, Razor starts one fresh dreaming session for each project whose sessions changed since its last pass, finishing each project before starting the next. Automatic discovery skips missing project roots and roots under the system temp directory, `/tmp`, `/var/tmp`, `$XDG_RUNTIME_DIR`, or the user cache directory (`$XDG_CACHE_HOME`, otherwise `~/.cache`). The first project that starts also tidies the memories shared by every project on that server; the remaining dreams list only their own project's memories. A restart during the night can repeat that shared tidy. A resident's project uses a fresh run; other projects get a top-level session named `dream · <date>`. A dreaming session closes at the end of a turn that leaves no input unanswered, including a turn opened by your answer. The pass uses the server's default model and effort, with `[memory] model` overriding the model and `[memory] night` changing the local time. These two settings belong in your own file or a pack, never in a project. Set `night = false` to turn dreaming off, including manual runs.

The session list starts at the later of the last pass and seven days ago; after a missed week, older session inputs fall outside the pass, while stored memories and state notes remain available. Dreaming reads the project's recent session digests, its memory contents and changed source citations. It checks claims, files knowledge into wiki pages, merges session state notes, looks for repeated failures and questions, and prunes what no longer helps. It can read files, recall sessions, load skills, read status, use memory and tasks, and send messages. It has no shell or file-writing tools. Memory permits ruling rewrites only as replacements of existing rulings, carrying forward all original quotations and citations; dreaming cannot originate a ruling. Each rewrite works from your original words, preserving the meaning of every ruling that still applies, and each night it checks new rules against their sources.

The dream's input lists current rulings by their rule line and id, including old-form and expiring files: up to 200 lines in 16 KiB, with a count of the rest. It reads the original words with `memory get`. Only the first nightly pass includes every-project rulings; a manual pass lists only the project's. Dreaming asks you only about real contradictions between your rulings. After changing rulings it leaves one project note: how many it merged, retired and reworded, with the affected ids and a citation to its session. The note expires after seven days and appears on the next session's startup page. A pass that changed none leaves no summary note. Its closing reply gives those counts alongside memories read, corrected and filed, and repeated rulings.

Run `razor dream [folder]` to start a project's pass now and print its session id. The folder defaults to the current folder. A manual run tidies only that project, can explicitly target an existing temporary folder, and refuses a missing folder before claiming a pass. With dreaming off, the command prints that it is off and starts no session. A stamp written before the start prevents a crash from repeating that night's pass; the sessions it was reading count as read only once the pass has ended, so the next night's pass reads them.

## Several machines

Each Razor home keeps its own memory unless a project names `[memory] remote` in `.razor/config.local.toml`. As with tasks, this may name a checkout remote, a git URL or a path. The project's memory folder becomes a checkout whose commits travel only as `refs/razor/memory`, independently of tasks and source branches. Memories for every project under `all/` remain local.

Writes, replacements and retirements are fetched, applied and pushed before they answer; a raced push retries against the remote's latest files. Replacing or retiring a memory already replaced elsewhere is refused. Replacements keep their scope: `all` must match the original memory. Reads keep working on the last fetched files while the remote is unavailable; writes are refused until it returns. Edits you make by hand are committed and rebased onto the remote’s files; a conflict refuses the call until you resolve it.

A shared project has one pass a night among a person's homes that can reach each other; a home none of them reaches runs its own. Each home's sessions are read by exactly one pass a night, none twice and none left unread. The first of your [paired servers](machines.md) to claim the night, through the same push, reads your sessions of the project from every one of them that answers; the others skip that night. A project is the same on two servers when their memory folders share a history, whatever its path on each. The stamp `.dream` in the project's memory keeps a line for each server: when its night was last claimed and how far its sessions are read. An unavailable server, or one whose required digests could not be supplied, keeps its old read mark and is read the next night. A server with no line has never been read, including with an old stamp. When one of your servers has an always-on agent in the project, the others leave the night to it, since only its pass tidies that agent's own memory; they run the pass themselves when it does not answer. A pass never reads another person's sessions: their own servers run their own pass.

```toml
[memory]
remote = "ssh://git@git.example.com/you/app.git"
```

A home whose memory or tasks folder already has its own files and a different history keeps that folder local and gives a warning on the startup page: to line it up by hand, move it aside, make a call to take the shared files, then copy back memories under unused names or recreate tasks with the task tool.

Only the project's private settings may set this key. Choose a repository whose readers may see the project's memory. With no remote named, the memory folder remains plain files.

### Sharing with other people

Each shared change, to memory or tasks, is committed as you: with `user.name` and `user.email` as git reads them in the project's checkout. Until both are set, a shared write is refused and nothing changes; a project that shares nothing never needs them. The author of the commit that added a ruling is its owner. In a shared project each ruling's line on the startup page names its owner and marks your own, and only the owner can retire or replace it, dreaming included. When people disagree, each records their own ruling, both show, and each person's agents follow their own person's.

To record an agreement, write it as a new ruling citing your words, with your old ruling in `replaces` and the other person's ruling ids in `settles`. Those rulings are set aside at once, and everyone follows the agreement. When the other person next opens the project, their page asks them about each of their rulings set aside: yes retires it; no replaces it with a fresh copy, which stands again. Other work goes on meanwhile.

A repository you cloned may already share memory or tasks on its origin. Once per project, in the background and never while a session starts, Razor asks the origin; when it holds either, a later startup page asks you whether to join. Yes sets both remotes to `origin` in `.razor/config.local.toml`; no sets `[memory] join = false` there, so you are not asked again. Nothing is shared until you say yes. A page asks at most five such questions, then counts the rest.

Until project memory folders are isolated, shared memory refuses a directory outside its defined structure: at the top, any directory besides `state`, `wiki`, `residents` and `.git`, and inside `state` or `wiki`, any directory. The refusal names it: move it out of the project's memory folder, or remove `[memory] remote` from `.razor/config.local.toml` to keep the memory local. This check runs before every shared change and refresh, so a folder added after sharing also stops the next push. It does not remove anything already pushed. A nested project under `state`, `wiki` or `residents` can be indistinguishable from the parent's own files and remains a limit.

## Tasks

Each task store serializes its own writes; a slow write in one project leaves other projects' task stores available.

A project's tasks are the work still to do: each has a number, a title, a description and a priority (0, most urgent, to 4), and may have a parent task, tasks blocking it, a due date and a claim. Agents keep them with their `task` tool; anything owed, an agent's promises to itself included, is a task with a due date.

- A **claim** says which session is on a task, and sets that session's headline to the task's title. It ends when the session closes with `/done` or `/quit`. A parent can start a helper on a task in the helper's project on its server; when it settles the helper, the helper's report becomes a comment on the task, and merging a finished helper closes it. When a parent closes, Razor stops and discards every unsettled helper, preserving its uncommitted work on its branch before removing its worktree; this releases its claims and leaves unfinished tasks open. An unreachable paired helper or failed preservation stays unsettled. Idle close keeps helpers available. Helpers can read tasks, claim or release existing ones, and comment in their own project on their server; claims end at settlement. A helper on another server keeps its claim until it is settled; if its parent closed while that server was out of reach the helper stays unsettled, and reopening the parent and closing it again settles it, or the person releases the claim.
- A task may carry `resident:` with an always-on agent's name; the task stays in the list, and only that agent's sessions see it on their startup page.
- When a task closes, its header records `closed:` with the date, time and reason, and `closed-by:` with the closing session's id, server id, name and machine, in a claim's form. A close made by settling a helper names the parent that settled it; the report's comment names the helper. Older closed tasks may have no `closed-by:` line. Each session holding a task it blocked gets a note. The day after a task's due date, the session holding it, or else the session that added it, gets one reminder if that session is open; a new due date allows a new reminder.

You see a project's tasks in the terminal app, read-only; to change one, tell an agent.

- The status pane's TASKS section shows the project of the session on screen: the tasks that session holds, then overdue ones, then the rest, three at first, with `▸ N more · ✓ k done this session` under them. Clicking that line lists every open task, fifteen to a page, then the ones this session closed.
- `/tasks` lists the same tasks grouped under headings: held by this session, ready, in progress, blocked, with open parts, and done this session. A click on a task, or `/tasks <number>`, opens its card: state, claim, priority and due date, the tasks blocking it, its parts, its description and its newest comments. Esc goes back to the list.
- Both follow changes as they land, from any session or machine sharing the list; programs read the same lists through the API ([Scripting](scripting.md#the-local-api)).

Each project's tasks are one folder in `~/.razor/tasks/`: a folder for each part of the project's path (with `%` written `%25`), then a folder named `%`, so a nested project's tasks never sit inside its parent's. An open task is `<number>.md`; a closed one moves to `closed/`; comments are files under `comments/<number>/`:

```text
~/.razor/tasks/home/you/code/app/%/
  12.md
  closed/9.md
  comments/12/20261102-161200-0a1b2c3d.md
```

To share a project's tasks between your homes and machines, or with people working on the same repository on their own, name a remote in the project's private settings file, `.razor/config.local.toml` (only that file may set it): a remote of the checkout, or a git URL or path (a relative path is read from the checkout). A URL keeps the tasks where you put them when the checkout's remote later moves elsewhere, a public repository included; a name follows the remote wherever it goes.

```toml
[tasks]
remote = "ssh://git@git.example.com/you/app.git"
```

Anyone who can read the repository can fetch its tasks, so name a remote only for a repository no stranger can read.

The tasks folder then becomes a git checkout whose commits go only to the hidden reference `refs/razor/tasks` on that remote, never to a branch, so they start no CI run and never reach a pull request. Each change is applied to the remote's tasks as fetched within the last 30 seconds, committed and pushed before it answers, and if another machine pushed first Razor fetches and tries again, so a claim that lands is the only claim; a change is refused while the remote is out of reach, or while the project's settings do not read, and reading keeps working on what was last fetched. Edits you make by hand in the folder are committed and kept. A failed release from a closed root or locally settled helper is retried within a minute of the remote answering again. For a paired helper, the failed release fails settlement and retains its worktree; settle again when the remote answers. A project naming no remote is plain files and always writable. A session's startup page lists the tasks its home has fetched; in a shared project with no open task there, it says to run `task list`, since this home may not have fetched them yet.

### Questions for another person

In a shared list a task can be addressed to a person: `for:` with their name or email as their commits to the list carry it. Such a task is a question only they can answer. It shows among the questions on their startup page, with who asks, and on no one else's page; a name nobody sharing the list has is refused with the names it could be. An agent unsure what another person's term means takes the likeliest reading, writes that reading as a comment on the task and carries on. Each person's nightly pass is shown the questions for them with those comments, so it can correct a wrong reading in memory or the glossary; dreaming itself puts a question for another person in a task for them, never to you.

