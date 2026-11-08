# Memory

Each project has a memory that outlasts its sessions, so a new session starts knowing where the work stands and what you decided. Razor keeps it in plain Markdown files in its home, one file per memory, which you can read, edit and delete like any other file. It also keeps the project's tasks, the work still to do (see [Tasks](#tasks)).

## What a session starts with

Every session opens with its project's startup page, at most about 2,500 tokens. It holds the project's merged state note, then the newer notes sessions wrote when they closed with `/done`, newest first: the current work and the decisions behind it, together in at most 1,500 bytes. Each session keeps its own note, so parallel sessions preserve each other's handoffs. Session notes expire after 14 days unless given another date. Below it come your rulings, decisions you made in your own words, at most twenty lines: standing ones first, then those that expire, each newest first. Then the wiki pages' names and descriptions, followed by facts and notes written since the last dream, newest first, at most 24 lines together and then a count of the rest; and last, the tasks the session holds, the open tasks due within a week, and how many are ready.

Nothing else is added to a session's context by itself. During the work, agents search memory, read a memory whole, and write new ones with their `memory` tool, whenever you say "remember this" or they find something worth keeping.

## What is kept

| Kind | What it is |
| --- | --- |
| state note | A session's current work and decisions, merged into the project's state by dreaming |
| ruling | A decision of yours, quoted from your own message |
| fact | Something true about the project or the world that later sessions should know, with a one-line description |
| note | Anything else worth keeping, with a one-line description |
| wiki page | Lasting knowledge filed by dreaming, with sources and a description saying when to read it |

A memory belongs to its project, or to every project when the agent writes it that way. It can carry an expiry date, after which it is moved into the retired folder; a one-time answer of yours, such as a yes to one request, is recorded that way. A fact that changes is written again, replacing the old one.

## How your decisions become rulings

A ruling can only come from you. When an agent records one, it names the message of yours, or your answer to its question, that the ruling comes from, and Razor writes the ruling from the record itself: the question the agent asked, your words exactly as you wrote them, and the agent's reading of them in a line. An agent cannot write a ruling in its own words, and a ruling never cites another agent.

So when you settle something ("yes, every merge waits for the tests"), the agent can make it a ruling, and every later session in that project starts with your sentence. To retire a ruling, tell an agent; it retires it with the `memory` tool.

Two kinds of session write less. A helper (a child) writes no memory at all: it tells its parent what is worth keeping. A session in an always-on agent's folder, its threads and projects included, writes rulings, facts and notes there, but not the state note, which is the agent's home session's.

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

The project is the repository's main checkout, so a helper working in its own worktree shares its parent's memory; outside git, it is the folder the session started in. A memory's file name says its kind and when it was written, and its id is the name without `.md`. Each file starts with a short header (where the memory came from, any expiry date, and a fact's or note's description) between `---` lines, then the text:

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

Search matches the query's words as whole words, ignoring case, in a memory's text (not its header, so not a fact's description). Results are ranked by how many of the query's words each memory holds; among memories that hold the same number, the newest comes first. Newest here and on the startup page means most recently changed on disk, so a file you edit moves to the top. A search returns 8 results unless the agent asks for up to 20. Search reads the files each time and holds nothing in memory between calls. It was measured with 2,000 memories in a project and 2,000 for every project; past that, each search takes longer, because it reads every file, but there is no limit.

## Wiki pages

Dreaming keeps durable knowledge in `wiki/` under the project's memory folder, or `all/wiki/` for knowledge shared by every project. Each page has a `name`, a one-line `description` saying when to read it, and a `checked` date. The startup page lists these headers first, followed by facts and notes written since the last dream. Search and get include pages, whose ids are `wiki/<name>`.

Only dreaming writes pages through the memory tool, at most 200 pages in each wiki and 16 KiB per page. Pages cite the original sources and link with `[[name]]`. A filed fact is retired after its knowledge reaches a page. Conflicting claims remain visible with their sources; replaced values keep a dated history.

You can write `wiki/_instructions.md` to say what matters for a project. Dreaming reads it first and cannot change it. The file stays yours to edit.

## Sources

Facts and notes can name their sources with `cites`: session entries as `razor:<session>:l-<entry>`, code or design as `file:<path>@<commit>`, cached references as `ref:<path>`, fetched pages as `url:<url> <date>`, and obligations as `task:<number>`. Each citation is a header line beside `source`, which still names the entry that wrote the memory. Rulings continue to quote your words from the record.

For the nightly pass, Razor checks code citations against main and live branches. They are on main, changed since the cited commit, awaiting merge on a branch, or discarded. Discarding a helper counts even while its branch remains on disk. A missing cached reference is listed too.

## Dreaming

At 03:00, Razor starts one fresh dreaming session for each project whose sessions changed since its last pass, finishing each project before starting the next. A resident's project uses a fresh run; other projects get a top-level session named `dream · <date>`. The pass uses the server's default model and effort, with `[memory] model` overriding the model and `[memory] night` changing the local time. These two settings belong in your own file or a pack, never in a project. Set `night = false` to turn dreaming off, including manual runs.

The session list starts at the later of the last pass and seven days ago; after a missed week, older session inputs fall outside the pass, while stored memories and state notes remain available. Dreaming reads the project's recent session digests, its memory contents and changed source citations. It checks claims, files knowledge into wiki pages, merges session state notes, looks for repeated failures and questions, and prunes what no longer helps. It can read files, recall sessions, load skills, read status, use memory and send messages. Its instructions limit messages to one question per project per night when rulings need your decision. It has no shell or file-writing tools, and memory refuses its ruling writes and replacements. A ruling retirement is allowed only in a turn opened by your answer to its question.

If duplicate or one-time rulings need your decision, dreaming asks one question per project per night; if you say yes, it retires the rulings listed in the question, keeping their files in the retired tree. Otherwise it sends nothing. Its closing reply gives the counts of memories read, corrected and filed, and repeated rulings.

Run `razor dream [folder]` to start a project's pass now and print its session id. The folder defaults to the current folder. With dreaming off, the command prints that it is off and starts no session. A stamp written before the start prevents a crash from repeating that night's pass.

## Several machines

Each Razor home keeps its own memory unless a project names `[memory] remote` in `.razor/config.local.toml`. As with tasks, this may name a checkout remote, a git URL or a path. The project's memory folder becomes a checkout whose commits travel only as `refs/razor/memory`, independently of tasks and source branches. Memories for every project under `all/` remain local.

Writes, replacements and retirements are fetched, applied and pushed before they answer; a raced push retries against the remote's latest files. Replacing or retiring a memory already replaced elsewhere is refused. Replacements keep their scope: `all` must match the original memory. Reads keep working on the last fetched files while the remote is unavailable; writes are refused until it returns. Shared projects claim their nightly pass through the same push, so one home runs it.

```toml
[memory]
remote = "ssh://git@git.example.com/you/app.git"
```

Only the project's private settings may set this key. Choose a repository whose readers may see the project's memory. With no remote named, the memory folder remains plain files.

## Tasks

A project's tasks are the work still to do: each has a number, a title, a description and a priority (0, most urgent, to 4), and may have a parent task, tasks blocking it, a due date and a claim. Agents keep them with their `task` tool; anything owed, an agent's promises to itself included, is a task with a due date.

- A **claim** says which session is on a task, and sets that session's headline to the task's title. It ends when the session closes with `/done` or `/quit`. A parent can start a helper on a task; when it settles the helper, the helper's report becomes a comment on the task, and merging a finished helper closes it. Helpers can read tasks and comment on existing ones; on their parent's server they can also claim or release an existing task, with claims ending at settlement.
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

The tasks folder then becomes a git checkout whose commits go only to the hidden reference `refs/razor/tasks` on that remote, never to a branch, so they start no CI run and never reach a pull request. Each change is applied to the remote's tasks as fetched within the last 30 seconds, committed and pushed before it answers, and if another machine pushed first Razor fetches and tries again, so a claim that lands is the only claim; a change is refused while the remote is out of reach, or while the project's settings do not read, and reading keeps working on what was last fetched. Edits you make by hand in the folder are committed and kept. A claim that could not be released while the remote was out of reach is released within a minute of its answering again. A project naming no remote is plain files and always writable. A session's startup page lists the tasks its home has fetched; in a shared project with no open task there, it says to run `task list`, since this home may not have fetched them yet.

