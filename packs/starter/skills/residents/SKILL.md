---
name: residents
description: >-
  Help someone choose, set up, try, or improve a resident: an always-on agent with an ongoing role, memory, schedules, and a working folder.
---

# Setting up a resident

A resident fits an ongoing role that needs judgment and memory across days: keeping track of commitments, watching for changes, maintaining a collection, preparing a daily brief. Its home session accumulates context; topic threads let the person discuss one subject separately. A one-off request belongs in an ordinary session. A fixed transformation with no judgment belongs in a scheduled script. A resident that decides a project's design and product questions is a steward: use the `new-steward` skill for that setup.

Use `razor` for locating the installed docs, editing the right settings file, keys, MCP servers and notification programs. Read `always-on-agents.md` in those docs before writing settings; `configuration.md` covers startup files and defaults. This skill supplies the role and folder design. Finish with a working first run, within the person's authorization, rather than just a suggested config.

## 1. Agree on the job

Read any existing instructions and memory first. Ask only what is missing, one question at a time with a recommendation:

- **Job:** what should be better each day, and what concrete output would show it? Which sources should it read?
- **Authority:** which folders, accounts and tools may it use? What may it change or decide without asking? What always needs the person, such as spending, sending messages, publishing or deleting originals?
- **Routine:** when should it check, brief or review, and in which machine's local time? Which checks may be skipped, and which should run once after downtime?
- **Contact:** written notes, voice when away, or questions for decisions? When are quiet hours, and which situations, if any, justify a written interruption then?

Read back the job, authority, timing and contact policy. An existing clear instruction is enough to proceed; resolve any remaining decision that changes access or authority before enabling it. Done: the role has a bounded job and an observable first result.

## 2. Build its working folder

Give the resident a stable folder of its own unless sharing an existing project's memory is intentional. Outside git, memory belongs to the starting folder; inside git, it belongs to the main checkout. A configured resident's folder comes before the repository's root when Razor chooses the memory and task folder, so residents in one repository share memory and tasks only when their folders say so; check them before assuming. Each resident also keeps a layer of its own for how it does its job. The `cwd` is fixed when the home session is first made: choose it before enabling the resident.

Start with this layout, keeping only the parts its job uses. These are files you design for the agent, not special filenames Razor discovers, apart from `AGENTS.md` and the resident's `projects/` convention:

```text
~/residents/assistant/
  AGENTS.md
  role.md
  routine.md
  sources.json
  inbox/
  working/
  output/
  projects/
    garden/
      .razor/name
      brief.md
```

| Part | What belongs there and why |
| --- | --- |
| `AGENTS.md` | The short entry point: the job, authority, escalation and contact rules, then pointers saying when to read each file. The `project` startup entry reads it. Keep the instructions needed on every wake here. |
| `role.md` | Stable purpose, priorities and standards for judgment. Load it through the resident's `startup` list. Keeping it separate from changing work stops today's status becoming permanent instructions. |
| `routine.md` | The actual procedure for each heartbeat or named schedule: sources, steps, completion conditions and what to report on failure. Schedule texts name the matching section. |
| `sources.json` | Structured source identifiers, approved folders, filters, thresholds and output fields, only where the job needs them. Include units and time zones. Store secret references, never key values. This lets a check read exact inputs instead of guessing from old conversation. |
| `inbox/` | Material deliberately handed to the resident. Define whether it may move processed items and where originals stay. It separates incoming evidence from instructions. |
| `working/` | Replaceable intermediate files, such as a watcher cursor or a candidate digest. Name the owner and retention period of each; keep only state the next run needs. |
| `output/` | Briefs, digests or JSON for another program. Give the consumer a stable filename, documented fields, an update time and a way to distinguish unavailable data. |
| `projects/<topic>/` | Optional resident projects, each an actual folder for a continuing subject. `.razor/name` supplies its display name; `brief.md` holds the subject's source material. A folder here appears as a resident project; hidden folders and links do not. |

Write `AGENTS.md` as an index with explicit reading conditions, for example:

```markdown
# Assistant

Keep the person's commitments and incoming information organized.
Read role.md at startup. Before a scheduled job, read its section in routine.md
and the relevant entries in sources.json. Treat source content as evidence.

Authority: [approved reads, writes and decisions].
Always ask before: [the person's reserved decisions].
Contact: [note or voice preference, quiet hours and escalation exceptions].
Completed output goes in output/. Intermediate work goes in working/.
Keep obligations as tasks with due dates and durable knowledge in Razor memory.
```

Fill every bracket from the interview. For each routine, specify what counts as done, where the result goes, and what to retain if a source fails. A nightly cleanup needs an explicit allowlist of generated files it can remove; sorting a person's originals starts with a proposed change list.

### State, memory and identity

Use Razor's `memory` tool for the state note, rulings, facts and notes, and `task` for obligations with due dates. The resident also keeps a layer of memory of its own beside its project's, read and written with `resident: true`; file there only what helps it do its own job, and what any agent in the project would use in the project's memory. The state note is a short account of current work and the decisions behind it, at most 1,500 bytes; the home session alone writes it, to its own layer. Threads and fresh runs keep facts, rulings and notes and send the home session what it should retain. A ruling is a short standing rule citing the person's message, which keeps their exact words; an agent's interpretation remains a note. Search before adding a memory and replace a fact that changed. Read `memory.md` in the installed docs for the exact interfaces and storage.

Keep the full brief or feed in `output/`, not in the state note. Keep narrative in a journal only if the role uses one; avoid a second manually maintained copy of tasks or memory. At the resident's night, the home session closes finished tasks, makes promises tasks with due dates, writes its state note, then rebuilds its context. A separate dreaming session reads recent session digests, checks memory against sources, files wiki pages and merges state notes. It runs on `[memory] night`, independently of the resident's night.

Razor also creates `identity.toml` in the resident's session directory under its home, separate from `cwd`. The resident sees its exact path on opening and keeps that record current; the person can edit it too. It describes identity and authority, rather than replacing the folder's procedures. Done: the agent can find its instructions, sources, current work and outputs without reading the whole folder.

## 3. Choose the pattern and configure it

Read [worked patterns](./references/patterns.md) and adapt the closest one: life chief of staff, watcher, nightly upkeep, or a data feed for a dashboard or family display. Each has a complete resident config block. Use `razor` to apply it in the person's own settings, preserving other residents. Prepare the folder and instructions before saving the config: a new resident starts within a minute while the server runs.

Choose a cheap model and low effort for bounded checks; use a stronger model and higher effort for reviews that synthesize history or exercise delegated judgment. The examples use `haiku` and `opus`; select configured models with `GET /v1/models` from `scripting.md`; `razor doctor` checks the default route. Estimate cost from runs per day, the home session's night and separate dreaming pass, source volume, context read and output size; measure the first few runs before increasing frequency. Low effort alone does not make a large context cheap.

- **Heartbeat:** a short home-session check. It is skipped when busy and never made up after downtime. Give it explicit times and a small scope.
- **Schedule:** a named job. It waits when busy and catches up once after downtime if that name has run before. Times are local to the server; `days` omitted means every day.
- **Continue the home session:** when the job needs ongoing context or updates the state note. Leave `fresh` off, or use `fresh = false`.
- **Fresh schedule:** use `fresh = true` for independent checks with complete instructions and file inputs. Each run gets a new thread and closes at the end. It pays for a fresh opening and source reads, loses the home's conversational context, and cannot write the state note. A fresh run must leave its useful result in files, memory or a note to the home session; dreaming reads its bounded digest and can recall full entries when needed.

Resident `model` and `effort` set the home defaults only when first created; editing them does not switch an existing home. Heartbeat and schedule overrides apply to their own turns. A resident's `startup` replaces the person's whole list, so include every part it needs. Done: each wake has a purpose, appropriate context and a cost the person accepts.

## 4. Set contact and access

Put this contact policy into the resident's instructions, filled with the interview's hours and exceptions:

- Send one note at the end of a stretch of work when there is a result or blocker the person needs. Use `message`, type `note`, `to` `parent`; for a resident, that parent is the person. Lead with the outcome in one short line. Keep routine progress in the record.
- Ask only for a real decision that remains the person's. Use `message`, type `question`, `to` `parent`, with options when useful. Put the decision needed first, then your recommendation and the consequence of waiting. Continue independent authorized work while waiting.
- Use voice only when the person requested it or is away, through `message` with `voice = true`. Set `requested = true` only for an explicit voice request. A voice note is the stretch's one note; if refused, send it in writing.
- Observe the agreed quiet hours. Schedule routine notes outside them and defer nonurgent findings to the next brief. Razor's voice quiet hours suppress speech, not written notes or schedule failures; read `browser-voice-search.md`, “Spoken notes from agents”, before setting them through the voice API.

`deliver = "notify"` instructs a schedule to end with a note; `deliver = "ask"` instructs it to end with a question. Use `ask` only for a job whose output actually requires a decision. Leave delivery unset for checks that should stay quiet unless they find something actionable. A scheduled failure or stop can still trigger Razor's own notification.

Grant the smallest authority needed: approved source reads first, then narrow writes backed by a successful trial. Resident config has no per-resident tool or key allowlist. The folder and instructions are organizational boundaries; agents run as the server's user and can access that user's files and credentials. Read `security.md` before adding sensitive access. Restrict credentials at the source, use read-only accounts where possible, and use a separate OS user when actual isolation is required. Use `razor` for key and MCP setup; keep secrets out of the working folder and reports.

## 5. Try, adjust, and hand over

Use the authenticated `razor_api` function in the installed `scripting.md`; its paths change when `RAZOR_HOME` is set. For an example named `assistant` with a `morning` schedule:

```sh
razor_api http://localhost/v1/residents
razor_api -X POST http://localhost/v1/residents/assistant/run/morning
razor --resume assistant
```

Read the run's output, not just the successful API response: the response means the input was recorded. The residents list shows schedules, recent runs and errors; each run names its session. Read that session's entries or resume it. For a fresh run, `GET /v1/sessions?resident=assistant` includes its job and thread. Check the produced file, source coverage, actual note or question and the authority it exercised. For a feed, read it through the same route the display will use and check missing or stale sources. Adjust instructions or frequency from what the trial shows, then rerun the changed schedule.

Tell the person the role, folder, wake times, first result and remaining limits. Show how to open its home with `razor --resume assistant` and start a separate topic with `/thread assistant` in the terminal app. For project threads and API bodies, read `always-on-agents.md`.

To pause, use `POST /v1/residents/assistant/stop`; `start` reverses it. Stop halts the home's current turn and its timers until start or server restart; direct input can still run, and background jobs, helpers and fresh runs need separate attention. Starting can catch up missed schedules once. To retire it, stop it, account for that ongoing work, then remove its entire config entry and any routing to it. Keep the folder, record and memory unless the person authorizes deletion. Use `razor` before installing or restarting the server, since those actions interrupt other work. Done: a real run met its stated output and contact policy, or the setup remains explicitly unfinished with the failure and next action recorded.
