# Configuration

Razor's settings are TOML files that it rereads when you save them; a change applies at the next turn, with the few exceptions below. Most people keep their settings in one file, `~/.razor/config.toml`, and can change them with `razor config`. API keys go in a separate file, `keys.toml`, that only you can read, and sign-ins are kept by each provider's own login.

## Change a setting

Use `razor config` from your project's folder:

```sh
razor config get ui.blocks
razor config set ui.blocks half
razor config set ui.motion false
razor config set ui.keys.global.status '["Ctrl-Y"]'
razor config unset ui.blocks
razor config path
```

`get` prints the effective value as JSON, followed by its source file or `built-in default`. File paths in values are absolute. An optional setting with no value prints `null`; a named entry that has no override reports that no value is configured. For keyboard actions without an override, `/keys` in the terminal shows the inherited keys.

`set` saves to your own file, creating it when needed. Values are TOML: booleans, numbers, quoted strings, arrays or inline tables. If a value does not parse as TOML, it becomes a string, so `half`, `high` and `15m` work without extra quotes. Shell quotes keep arrays and values with spaces together. A dotted key follows TOML syntax; quote a segment inside it if the name itself contains dots.

`unset` removes an override from the selected file, allowing the value beneath it to apply; it does not hide entries inherited from a pack. `path` prints the selected file. Add `--project` to `set`, `unset` or `path` for the shared project file, or `--local` for the private project file:

```sh
razor config set children.max 8 --project
razor config set tasks.remote origin --local
```

The same restrictions below apply to these commands. Unknown keys and invalid values are refused before saving. Comments and unrelated settings stay in the file. When changing a field inside an inherited named entry, the command may copy the entry from a lower file, preserving its written values. It never copies personal or pack settings into a shared project file, or project settings into your file. In those cases it writes only the requested field, which the loader may refuse if the entry needs other fields. These commands start no server; a running server rereads the saved file as usual. A key starting `voice.`, such as `voice.daily_characters`, is instead one of the voice settings the server keeps, outside any file, and the command asks the server ([Browser, voice and search](browser-voice-search.md#spoken-notes-from-agents) lists them).

## Where settings live

Razor reads up to four files. Each later one wins over the ones before it:

| File | What it is for |
| --- | --- |
| The pack's `config.toml` | A pack is a folder of default settings, prompt files and skills. Razor ships one, the starter pack, which a new install uses. Its file, `packs/starter/config.toml` in Razor's install folder, holds worked examples of the common optional settings as commented lines. Don't edit it: an upgrade replaces it. |
| `~/.razor/config.toml` | Your settings. With `RAZOR_HOME` set, it is `$RAZOR_HOME/config.toml`. |
| `<project>/.razor/config.toml` | A project's shared settings, at the root of its git repository (or the folder the session started in, outside git). |
| `<project>/.razor/config.local.toml` | The same project's private settings for this machine. It sits in the checkout, and an MCP server's `env` values are stored in it as written. When Razor creates it in a git checkout, as `razor mcp add` does, it adds the file once to the main checkout's `.git/info/exclude`, so it stays out of git without a change to any tracked file; Razor never edits `.gitignore`. If you create the file yourself, add it to `.git/info/exclude` or `.gitignore`. |

A helper working in its own worktree reads the project files of the main checkout. A folder whose `.razor` is Razor's home, such as your home folder, has no project files: its `.razor/config.toml` is your own file, read once.

## How the files combine

1. A plain value replaces the one beneath it: `model = "sonnet"` in your file wins over the pack's.
2. A table merges one level deep: `[children] max` in your file keeps the pack's `[children] grace`.
3. An entry with a name of its own, such as `[mcp.files]` or `[residents.scout]`, comes whole from the last file that has it. So does any table two levels down, such as `[profiles.codex]`: yours replaces the pack's whole.
4. A list replaces the list beneath it and is never joined to it.

## Paths in settings

A setting that names a file or folder (`pack`, `skills`, `startup` entries, a variant, key or secret file, a resident's `cwd`) counts a relative path from the folder of the file that names it, and `~/` from your home folder. So to change the starter pack's `startup` list, copy the whole list into your file and copy the files it names (`persona.md`, `rules.md`) beside your file. Two exceptions: a program in `ask`, `notify`, `speech`, `transcribe`, `capture` or `play` named without a `/` is looked up on `PATH`, and a plugin's relative `file` counts from Razor's home.

Commands are not rebased. An MCP server's `command` starts in the project's folder, so give it an absolute path or a program on `PATH`; a scripted tool's `command` is run by bash in the session's folder.

## What only your own file may set

Settings about you or the machine may be set only in your own file, `~/.razor/config.toml`, or in a pack, never in a project's files. A project file that sets one is ignored whole, and Razor keeps that file's last good settings, with a message naming the file and the keys, such as `/home/you/code/app/.razor/config.toml cannot set ui; move these keys to the person's file.` `razor config`, `razor mcp` and `/mcp` refuse to change such a file and show that message; nothing else shows it yet. These are the settings:

- the server and models: `server`, `release_updates`, `remote`, `providers`, `provider_only`, `models`, `claude`, `responses.base_url`, the whole `[sessions]` table, `call_model`, `call_effort`;
- what conversations start with: `startup`, `children.startup`, `skills`, `skill_updates`, `pack`;
- programs Razor runs for you: `ask`, `notify`, `speech`, `transcribe`, `capture`, `play`, `plugins`;
- safety: `guards`;
- always-on agents: `residents`;
- display: `brand`, `ui`;
- `phone`, `push` and `live`, which serve a separately distributed phone client (`live` sets where its voice calls go: OpenAI by default, or `url`, and `names`, the names its voice will hear).

`[tasks]` is the other way round: only a project's `.razor/config.local.toml` may set it ([Tasks](#tasks)).

## When a change applies

- Most changes apply at the next turn.
- `server.address` applies when the server restarts, and Razor says so when you change it ([Troubleshooting](troubleshooting.md#restarting-the-server) says how to restart).
- `keys.toml` is read when the server starts, so run `razor restart` after `razor keys`.
- A file that does not parse, names an unknown key, or holds a wrong value, is ignored, and Razor keeps that file's last good settings. For your file and the pack's, the terminal app's status pane and `server.log` name the file and the problem.
- A server whose own file or pack is broken does not start, and `server.log` says why.
- A settings file is at most 1 MiB.

## Keys

API keys are read from the server's environment, and `~/.razor/keys.toml`, readable only by you, holds the ones you save with Razor: one line each, the variable's name and its key. When the server starts it sets each variable its environment lacks from that file, so an exported variable wins, and agents' shells and MCP servers get none of the names the file lists. Setup's API key choice writes a provider's key there, and `razor keys` asks for the optional ones:

| Variable | Used for |
| --- | --- |
| A provider's `key`, such as `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, `GEMINI_API_KEY`, `RUNINFRA_API_KEY` or your gateway's | That provider's models ([Models and sign-in](models.md#providers)); `OPENAI_API_KEY` also carries the phone client's voice calls |
| `EXA_API_KEY`, `PARALLEL_API_KEY`, `FIRECRAWL_API_KEY` | The web tools `search`, `fetch` and `answer` ([Browser, voice and search](browser-voice-search.md)) |
| `ELEVENLABS_API_KEY` | The bundled speech program; a restricted key needs the Text to Speech and Voices (read) permissions |

A name is letters, digits and `_`, not starting with a digit, and each key a nonempty string; anything else stops the server and `razor doctor` from starting, with a sentence saying so. If you write the file yourself, make it private with `chmod 600 ~/.razor/keys.toml`, then run `razor restart`:

```toml
# ~/.razor/keys.toml
EXA_API_KEY = "your-exa-key"
GATEWAY_API_KEY = "your-gateway-key"
```

Model sign-ins are kept by each provider's own login ([Models and sign-in](models.md)).

## Terminal keys

Every terminal keyboard action can be changed in your own `config.toml`. Tables name the context, and each action has a list of keys:

```toml
[ui.keys.global]
prefix = ["Ctrl-A"]
status = ["Ctrl-Y"]

[ui.keys.prefix]
detach = ["q", "d", "D"]
```

Missing actions keep their defaults; `[]` unbinds an action. `overview` in `global` (F3 by default) opens the agents overview and leaves it again; the overview takes the `list` keys, with `up` and `down` moving between agents and folded groups, `right` opening the selected agent or unfolding the selected group, `fold` folding the selected agent's group, `left` doing nothing, and `group_1` to `group_9` (the keys 1 to 9) selecting a numbered top-level group, as they do in the list. `outward` in `line_start` (Left at the start of a composer line) goes from a helper to its parent, and from any other session opens the agents list beside it, then the overview; `inward` in `draft_end` (Right at the end of the draft while the list is open) closes the list. `outward` was named `list` until November 2026, and a setting that still names `line_start.list` is refused as an unknown action, keeping the terminal's last valid keys. While more than one question waits, Left and Right at an empty draft are the `question` context's `previous` and `next` instead, stepping between the questions.

### Moving between agents

By default the arrows in the agents list move its highlight row by row: Up and Down (or `k` and `j`) step through every row, helpers included; Left (or `h`) moves to the parent agent or heading; Right (or `l`) unfolds a heading or an agent's helpers and moves into them; Enter opens the selected agent and closes the list. F2, or the prefix and `h`, gives the list the keys, and the prefix and Tab moves them between the list and the conversation.

Unbound by default, these actions instead give a loop for moving between agents and typing to each one from the keyboard alone, the list staying open:

```toml
[ui.keys.list]
up = []
down = []
left = []
right = []
accept = []
previous_agent = ["Up", "k"]
next_agent = ["Down", "j"]
overview = ["Left", "h"]
beside = ["Enter", "Right", "l"]

[ui.keys.line_start]
outward = []
choose = ["Left"]

[ui.keys.composer]
switch = ["Shift-Tab"]
```

- **To the list.** `choose` in `line_start` (Left at the start of a composer line) and `switch` in `composer` (Shift-Tab anywhere in the draft) give the list the keys, opening it if it is closed, with the agent you are talking to selected. Your draft stays in its composer. In a helper's conversation, `choose` goes to its parent first; `switch` goes straight to the list. While you browse the click targets, Shift-Tab still steps back through them.
- **In the list.** `previous_agent` and `next_agent` step through the top-level agents, resident threads and headings, passing over helpers except one that needs you. `beside` opens the selected agent beside the list, its composer taking the keys, ready to type, or folds or unfolds the selected heading. `overview` opens the overview. `switch` gives the keys back to the conversation you were in.

With either set, Right at the end of the draft while the list is open (`inward` in `draft_end`) closes the list, leaving the conversation on the whole screen, as F2 does, and your draft stays where you typed it. Ctrl-N and Ctrl-P in a parent's conversation select its helpers' cards, beside the list too, and Enter opens the selected one. The list's last line names the keys of whichever of these actions are bound.

### The prefix

Many actions sit behind a prefix, as in tmux: press backtick (`` ` ``) or Ctrl-B, then the action's key. The prefix works only while the draft is empty, so a backtick among your words is typed and Ctrl-B there moves the cursor left. Pressed twice, the prefix sends itself: two backticks type one. Esc cancels the prefix, and a key with no prefix action is refused with a line saying so.

| After the prefix | Action in `[ui.keys.prefix]` | What it does |
| --- | --- | --- |
| `j` / `k` | `next_project` / `previous_project` | Opens the next or previous project's first top-level agent |
| `!` `@` `#` … `(` (Shift-1 to Shift-9) | `project_1` … `project_9` | Opens the first top-level agent of project 1 to 9 |
| `n` / `p` | `next_agent` / `previous_agent` | Opens the next or previous top-level agent in this project |
| `1` to `9` | `agent_1` … `agent_9` | Opens top-level agent 1 to 9 in this project |
| `c` | `global.new` | Starts a new agent |
| `T` or `,` | `rename` | Types `/name ` to rename this session |
| `X` | `done` | Types `/done`; Enter closes this session after its last turn |
| `v` or `-` | `split` | Opens the selected child card, or the first child, in a split |
| `x` | `close_split` | Closes the child's split |
| `h` / `l` | `left` / `right` | Gives the keys to the agents list / to the conversation |
| `Tab` or Shift-Tab | `switch` | Moves the keys between the agents list and the conversation |
| `H` / `L` | `narrower` / `wider` | Narrows / widens the agents list |
| `w`, `b` or `s` | `list.open` | Opens or closes the agents list |
| `g` | `goto` | Opens Go to, as `/goto` does: find any agent or past conversation and open it |
| `o` | `waiting` | Opens the oldest question waiting for you, else what the status chip opens |
| `[` | `browse` | Browses the click targets from the first, as Tab does: `j`/`k` or the arrows move, Enter opens, Esc leaves |
| Up / Down | `previous_reply` / `next_reply` | Selects the agent's reply above or below on screen |
| `?` | `keys` | Lists every key |
| `q` or `d` | `detach` | Leaves the terminal; sessions keep running |

A reply is selected at its first row, or at the copy line under a long one. While browsing, `"` quotes the selected reply into the draft, `y` copies it (or the selected link), and Enter opens it read-only in your own editor rather than pinning it, which a click on its first row still does. The editor is `$VISUAL`, else `$EDITOR`, else `vi`, its words quoted as in a shell but run without one (no `$` expansion), with `-R` for vi, vim and nvim, so your own configuration and motions apply. Razor waits until you leave the editor, then removes the reply's temporary file.

An entry in `[ui.keys.prefix]` may also name any other action in full, which then runs after the prefix as its own key does: the shipped `c` is `"global.new"` and `w`, `b` and `s` are `"list.open"`. To open the status pane with the prefix and `t` and free Ctrl-T:

```toml
[ui.keys.prefix]
"global.status" = ["t"]

[ui.keys.global]
status = []
```

Projects follow the agents list: its top-level agents in the list's order, whatever heading holds them, grouped by the folder each runs in, leaving out Earlier and each NEEDS YOU row that another heading lists. Opening an agent this way opens the list beside it, and at either end `j`/`k` and `n`/`p` stop.

Inside tmux, Herdr or another program whose own prefix is the same key, that program takes the key first. Press its prefix twice to send it on to Razor (tmux's `send-prefix`), or give Razor another prefix in `[ui.keys.global]`.

### The draft

With words or images in the draft, Ctrl-C empties it (`composer.clear`), and the undo key brings them back; with nothing to send, Ctrl-C starts quitting as before. Ctrl-S (`composer.stash`) sets the draft aside, words and images, and empties the box; Ctrl-S with an empty draft brings it back, into whichever conversation is open then. One draft is held at a time, until the terminal app quits, and stashing another while one is held is refused.

Each conversation keeps your unsent words, up to 1 MiB, as its draft on the server, so they are there when you come back to it, from this terminal app or another. A draft holds words only: images in the box are dropped when you leave the conversation, and the terminal says so. Moving to another agent waits until the server has saved your words; if it cannot, you stay where you are, with your words and a sentence saying why. Words typed for a new agent or thread wait in the terminal app instead, for up to eight new agents at once, and come back when you return to start that same one. Only the terminal app holds them, so quitting it loses them; the first Ctrl-C of a quit names them.

### Vim keys

Panels such as `/keys`, `/help`, `/tasks` and `/status` scroll with `j`/`k`, by half a page with Ctrl-D/Ctrl-U, and to the top and end with `g`/`G`. In `/keys` and `/help`, `/` starts a search: type words and only the rows holding all of them stay; Esc ends the search, and Esc again closes the panel. The model chooser also moves with `h`/`j`/`k`/`l`. With an empty draft, Ctrl-D and Ctrl-U scroll the transcript half a page; with words in the draft they keep their editing meaning. A letter typed into an empty draft is always typed, so `j` and `k` move only while browsing.

### Contexts

Each table is a context, the place its keys apply: `global`, `transcript`, `list` (the agents list), `prefix`, `goto` (the Go to panel), `composer`, `line_start` (the start of a composer line), `draft_end` (the end of the draft while the agents list is open), `targets` (selecting click targets), `children`, `input_history` (earlier inputs), `queued_input` (withdrawing queued input), `attached_images` (entering the image selection), `question`, `voice`, `picker`, `panel`, `mcp`, `start`, `emoji`, `completion`, `shortcode`, `attachments` (while selecting an image), `editor` and `skill`.

Run `/keys`, or the prefix and `?`, for every action, its context and effective keys. The [shipped catalogue](https://github.com/treygoff24/razor/blob/main/src/terminal/data/keys.toml) supplies the defaults and descriptions in the same format. Keys accept `Ctrl-`, `Alt-`, `Shift-` and `Super-`, named keys such as `Enter`, `Esc`, `Space`, `Left`, `PageUp` and `F2`, or a single character. Each action accepts up to eight keys. Changes apply while the terminal is open, and hints update with them. An invalid name, key or duplicate within a context shows an error and keeps that terminal's last valid bindings.

## The file about you

`~/.razor/about.md` is plain Markdown that every conversation starts with, through the `about` part of the `startup` list. Razor does not make one: write it yourself, or ask an agent to, with a first line `# About <your name>`, then a few lines about you.

```markdown
# About Sam

I build web apps, mostly in TypeScript, and I prefer small pull requests.
```

## Every setting

Durations are a number and a unit: `90s`, `3m`, `2h`, `1d`. Times of day are `HH:MM` in local time.

### Models and sessions

| Setting | Default | What it does |
| --- | --- | --- |
| `model` | `"opus"` | The model new sessions start on ([Models and sign-in](models.md)). |
| `speed` | `"standard"` | New root sessions use `standard`, `fast` or `ultrafast`; `razor --speed` overrides it. Children start at Standard unless explicitly given a speed. `/fast` opens the speed row; `/fast on`, `/fast off` and `/fast ultrafast` stage the next input. The picker shows prices and unavailable reasons. |
| `effort` | `"high"` | How much work the model puts into each step in new sessions: `low`, `medium`, `high`, `xhigh` or `max`. |
| `call_model` | `"terra"` | The model a session started for a voice call runs on, such as the phone app's Talk; any model Razor can run. A call to a conversation already under way keeps that conversation's model. |
| `call_effort` | `"low"` | The effort of a session started for a voice call. The phone app can choose its own model and effort for calls, which win over these two. |
| `[rebuild] threshold` | `0.9` on a window of 500,000 tokens or more, else `0.8` | How full a context may get, as a fraction of its size, before Razor asks for a continuation note and starts a fresh one ([Concepts](concepts.md#when-the-context-fills)). Set, it applies to every model without its own threshold; `razor config get rebuild.threshold` reads `null` while it is unset. |
| `[sessions] handover_minutes` | `50` | Idle minutes before requesting a continuation note on every model route, residents included. Running turns and jobs hold it off; working children do not. `0` disables it. |
| `[sessions] handover_tokens` | `50000` | Minimum context size for an idle handover, for asking whether a model switch keeps the conversation, and for a switch's fresh note; `0` removes the floor. |
| `[sessions] close_minutes` | `0` | Idle minutes before a top-level conversation closes without a model turn. Residents' home sessions, children and sessions with pending work stay open. The next person input resumes the same conversation. |
| `[sessions] idle` | `4` | How many idle sessions keep their model conversation open. On Claude each one holds a Claude Code process; the longest idle closes first and reopens at its next input. |
| `[stuck] after` | `"15m"` | How long a working session may sit with unanswered input, no command running, no question open, no `agent wait` within its timeout and no working helper before it shows as stuck. |

### Model routes

These change how Razor reaches Claude and GPT; most people never set them. `[profiles.claude]` covers the Claude models and `[profiles.codex]` the GPT models, named after Codex, whose tools GPT is shown.

| Setting | Default | What it does |
| --- | --- | --- |
| `[profiles.claude] code_mode` | `false` | Code mode: instead of separate tools, the model gets one `exec` tool and writes JavaScript that calls Razor's tools, which lets it batch many reads in one step. This turns it on for Claude. |
| `[profiles.codex] code_mode` | `true` on the ChatGPT sign-in, `false` on a provider | The same for GPT models. It cannot be turned on for a provider's model, such as `openai/gpt-6.1-sol`. |
| `[profiles.codex] image_gen` | `true` | On the ChatGPT sign-in, gives GPT Codex's image generation, which makes and edits images on your subscription, and tells other models, a provider's GPT aside, to use a GPT helper; not with `provider_only` on. Set `false` to remove the helper sentence and leave the 2,500-byte tool description out of GPT conversations. It takes effect at the next turn; a project may set it. |
| `[responses] base_url` | `"https://chatgpt.com/backend-api/codex"` | Where requests on Razor's ChatGPT sign-in go. GPT on an OpenAI API key is a provider's model instead, such as `openai/gpt-6.1-sol` ([Models and sign-in](models.md#with-an-openai-api-key)). |
| `[claude] sign_in` | `[]` | A command and its arguments that print a Claude sign-in token. Razor runs it for each conversation it opens on Claude, so a script of yours can pick the account. Empty: Claude Code's own sign-in. |
| `[claude] usage_file` | none | A JSON file that another program keeps with each Claude account's plan usage; Razor reads it to show the usage of the account `sign_in` picked. |

### Providers

`[providers.<name>]` is one provider: where its models are reached, with the variable holding its key. Its models are named `<name>/<id>`. A table named after a shipped provider (`anthropic`, `openai`, `google`, `runinfra`) changes only the fields it sets ([Models and sign-in](models.md#providers)).

| Setting | Default | What it does |
| --- | --- | --- |
| `format` | the shipped provider's, else `"chat"` | The API it speaks: `"chat"` (OpenAI-compatible chat), `"responses"` (OpenAI's Responses API) or `"anthropic"` (Anthropic's Messages API, through Claude Code). |
| `base_url` | the shipped provider's, else required | Its address; Razor posts to `<base_url>/chat/completions` or `<base_url>/responses`, and Claude Code to `<base_url>/v1/messages`. |
| `key` | the shipped provider's, else none | The name of the variable holding its key, saved in `keys.toml` or exported ([Keys](#keys)). Never the key itself. With none, Razor sends no key. |
| `models` | `[]` | The model ids `/model` lists for it. |

`provider_only = true` makes Razor itself reach only the providers named under `[providers]` ([Security](security.md#contacting-only-your-model-provider)). It is `false` by default, and only your own file or a pack may set it.

### Models

`[models."<name>"]` changes how Razor treats one model, a shipped one or a provider's. For Claude and GPT only `window` and `threshold` may be set; the rest are for chat models.

| Setting | Default | What it does |
| --- | --- | --- |
| `speed_ids` | empty | Chat model variants, for example `speed_ids = { fast = "accounts/fireworks/routers/kimi-k3-fast" }`. The provider, credentials, profile, effort and context limits stay the same. Missing variants are unavailable. Gemini Fast uses its priority tier directly; a conflicting `request.service_tier` is refused. |
| `window` | the shipped or reported size, at most 500,000 | The context size in tokens. Claude Code reports 1,000,000 for Opus, Sonnet and Fable, and Razor fills 500,000 of it; set `window = 1000000` to use it all. Razor does not guess one for a provider's model, so set it. |
| `threshold` | `[rebuild] threshold` | This model's own rebuild threshold. |
| `replay` | `"streamed"` | How the model's earlier reasoning is sent back with the next request: `streamed` as it arrived, `reasoning` or `reasoning_content` in the field of that name, which some hosts require, or `none`. |
| `request` | none | Extra fields sent with every request, such as an option only that host has. |
| `variant` | the shipped row's, else four plain tools | A JSON file describing the tools this model is shown, in place of the default. |

### The system prompt and skills

These decide what every conversation is told before your first message, and which skills, folders of written instructions, its agent can load.

| Setting | Default | What it does |
| --- | --- | --- |
| `startup` | `["razor", "project"]`; the starter pack's is `["persona.md", "razor", "about", "project", "rules.md"]` | The parts of a top-level conversation's system prompt, in order. Three words are not files: `razor` is Razor's own text about its tools, `project` is the `AGENTS.md` and `CLAUDE.md` files from the repository root down to the session's folder, and `about` is `~/.razor/about.md`. Anything else is a file, or a folder whose first 64 `.md` files are read. |
| `skills` | `[]` | More folders of skills: written instructions an agent loads by name, one folder per skill with a `SKILL.md`. These are searched last, after the folders below. |
| `skill_updates` | `true` | Update eligible machine skills on the first server wake and then daily. Set `false` to stop future runs and restore shipped pack skills in lookup; an update already running finishes. With `provider_only` on, none runs. |
| `pack` | the starter pack | The pack folder. Naming one replaces the starter pack; removing the name brings it back. |

Razor looks for skills in this order, and when two folders hold a skill of the same name, the first one wins:

1. Machine-owned skill overrides in `<Razor home>/skills`.
2. The pack's `skills` folder.
3. The project's `.agents/skills` and `.claude/skills`.
4. Your own `~/.agents/skills`, `~/.claude/skills` and `~/.codex/skills`, where Codex and Claude Code keep your personal skills, so the skills you already use there come with you.
5. The folders the `skills` setting names.

A new conversation's opening lists the skills of the first four with their descriptions, up to 100, filled in that order. The `skills` setting's folders stay available by name and through slash-command search. A skill of yours that shares a name with the pack's or the project's is not used; rename it to use both.

Automatic updates cover your three home skill folders and folders named by `skills`; project skills stay with their repositories. Git checkouts advance only when clean, including ignored files, on a branch tracking a remote, and only by fast-forward. This advances the whole checkout, including files outside its skills folder.

For installations named in the skills CLI's global lock, Razor runs `npx --yes skills@1.7.6 update --global --yes <names>`. Node 22.20 or later is required. The vendor may overwrite local edits and refresh installations for other detected agents. Keep tailored instructions under another name outside the vendor lock or in the project, or disable updates.

Pack skills with a branch in their `UPSTREAM` file receive an override in this Razor home. The pack remains untouched; pinned tags and commits move with Razor releases. Overrides use a stable symlink, retaining the current and previous version. Failures and skipped updates appear in `server.log`, with no agent turn or notification. See [automatic skill updates](design/skill-updates.md) for bounds and provenance.

### Helpers (`[children]`)

An agent can start a helper as a reviewer or a researcher, which gives it Razor's instructions for that role.

| Setting | Default | What it does |
| --- | --- | --- |
| `[children] startup` | `["project"]` | The system prompt for every helper, including a resident's helper, using the same entries as `startup`. Independent of the parent's list; `[]` sends only the model route's mandatory text. Only the pack and your own file may set it. A remote helper uses its destination server's list and files. Read again at a fresh opening or rebuild; a native resume retains its recorded prompt. |
| `[children] max` | `20` | How many working helpers one session may have at once. |
| `[children] grace` | `"3m"` | How long a helper that stopped without reporting is given before Razor asks it once to finish. |
| `[children] reviewer` | none | A file whose text replaces Razor's instructions for a helper started as a reviewer. |
| `[children] researcher` | none | The same for a helper started as a researcher. |
| `[children] watcher` | `"gemini-3.8-flash"` | The model of the watcher: a helper Razor starts by itself when a client sends a video (the terminal app does not) to a session whose model cannot watch video, and which reports a description and a timeline of key moments. Its key goes in `keys.toml` or the server's environment like any other model's ([Keys](#keys)); for the default that is `GEMINI_API_KEY`. With `provider_only` on, no watcher starts. Without the key no watcher starts, and the session gets only the video's file path. |

### Guards

| Setting | Default | What it does |
| --- | --- | --- |
| `[guards] delete` | `true` | Refuses an agent's command or patch that would delete a file that existed before its session started. A check of the command's text, not a sandbox ([Security](security.md#what-agents-can-run)). |

### Notifications

| Setting | Default | What it does |
| --- | --- | --- |
| `notify` | `[]`; the starter pack's is Razor's `scripts/notify.sh` | A program and its arguments that carry an agent's note to you; the note is its last argument. |
| `ask` | `[]`; the starter pack's is Razor's `scripts/notify.sh` | A program that carries a question to you, as its last argument. If it prints something and exits 0, that is the answer; if it prints nothing, the question waits for you in a client. |

The starter pack's `notify.sh` shows a desktop notification through `osascript` on macOS or `notify-send` on Linux, and does nothing on a machine with neither. On macOS the notifications come from Script Editor, which you may have to allow once in System Settings. To reach your phone instead, install the [ntfy](https://ntfy.sh) app, subscribe to a topic no one could guess, and put it in both lines:

```toml
# ~/.razor/config.toml
notify = ["curl", "-fsS", "-o", "/dev/null", "https://ntfy.sh/your-unguessable-topic", "--data-raw"]
ask = ["curl", "-fsS", "-o", "/dev/null", "https://ntfy.sh/your-unguessable-topic", "--data-raw"]
```

`-o /dev/null` matters: without it, ntfy's reply would become the question's answer.

### Voice

| Setting | Default | What it does |
| --- | --- | --- |
| `speech` | `[]`; the starter pack's is `scripts/speech.py` | A program that reads a reply's text on standard input and writes MP3 audio to standard output, for replies read aloud and agents' voice notes. |
| `transcribe` | `[]` | A program that reads a voice note's audio on standard input and writes its words to standard output. Empty: recording is refused. |
| `capture` | `[]` | A program that writes 16 kHz mono 16-bit PCM from the microphone to standard output. Empty: no recording. |
| `play` | `["afplay", "-r", "{rate}"]` on macOS, `[]` elsewhere | The player; `{rate}` becomes the speed, and the audio file's path is added last. |

[Browser, voice and search](browser-voice-search.md) has working values for each platform. With `provider_only` on, Razor runs neither `speech` nor `transcribe`.

### Web

| Setting | Default | What it does |
| --- | --- | --- |
| `[web] search` | `[]`; the starter pack's is `["exa", "parallel", "firecrawl"]` | The providers the `search` tool tries, in order. |
| `[web] fetch` | `[]`; the starter pack's is `["firecrawl", "exa", "parallel"]` | The same for `fetch`, which reads a page. |
| `[web] answer` | `[]`; the starter pack's is `["exa", "parallel"]` | The same for `answer`, a cited answer. Firecrawl has none. |
| `[web.<provider>] base_url` | the provider's public address | Another address for `exa`, `parallel` or `firecrawl`, such as a proxy. |

A web tool appears only while one of its providers has a key, and never with `provider_only` on. With none, agents keep their model's own web tools.

### MCP servers

`[mcp.<name>]` is one MCP server. `razor mcp add` and `/mcp` write these entries for you; `razor mcp add` writes to the current project's private file, `.razor/config.local.toml`, which Razor keeps out of git ([Where settings live](#where-settings-live)), unless you pass `--scope global` (your file) or `--scope project`, the shared file others who clone the repository get; in a folder whose `.razor` is Razor's own home, which has no project files, every scope writes your file. Any other folder, even an empty one outside git, gets its own `.razor/config.toml` or `config.local.toml`.

```sh
razor mcp add files --scope global --config '{"command": "npx", "args": ["-y", "@modelcontextprotocol/server-filesystem", "/home/you/notes"]}'
razor mcp list
```

`razor mcp list` shows each server's scope and whether it is on (`configured`) or off (`disabled`); whether it starts, and its tools, show in `/mcp` in a session.

In the terminal app, `/mcp` opens the list of servers, with their state and tools; `/mcp add <name> <url>` adds one reached over HTTP (`--header NAME=env:VAR` for a header read from the server's environment), and `/mcp add <name> -- <command> <args…>` one that runs as a program. To bring over the servers you already set up in Claude Code, Codex or OpenCode, type `/mcp import <file>` with that program's settings file, such as `~/.claude.json`, a project's `.mcp.json` or `~/.codex/config.toml`: Razor shows what it would add and writes it when you press Enter, leaving out any field that holds a secret for you to fill in. `razor mcp import <file>` shows the same preview without writing anything. `razor mcp sign-in <name>` starts a server's sign-in, and if the browser then lands on a page that does not load, `razor mcp complete '<that address>'` finishes it.

| Setting | Default | What it does |
| --- | --- | --- |
| `command`, `args`, `env` | none | A server that runs as a program, with its arguments and extra environment. It starts in the project's folder. |
| `url` | none | A server reached over HTTP or HTTPS instead. |
| `headers` | none | HTTP headers, each `{ env = "VARIABLE" }` or `{ file = "path" }`, so no secret is written in the file. |
| `oauth` | none | `issuer`, `client_id` and `client_secret` (as `{ env = … }` or `{ file = … }`) for a server that needs a registered client. Other servers sign in on their own: Razor sends you the link. |
| `enabled` | `true` | `false` turns the server off, including one a file beneath defines. |
| `search` | automatic | Whether agents find this server's tools through a search instead of seeing them all at the start. Automatic means yes once its tool declarations pass 8 KiB. |
| `timeout` | `600000` | How long one tool call may take, in milliseconds. |
| `tools` | all | The only tools this server offers, by name. |

A project's servers start without asking when a session opens there.

### Script tools

`[tools.<name>]` turns a script into a tool. Each call runs the command with bash in the session's folder, with the call's input as JSON on standard input; the output is the result.

```toml
[tools.forecast]
command = "~/bin/forecast"
description = "Today's weather forecast for one city."
input = { type = "object", properties = { city = { type = "string" } }, required = ["city"] }
read_only = true
```

`read_only = true` lets it run beside other read-only calls in the same batch. `input` must be a JSON schema whose top is `{ type = "object" }`.

### Plugins

`[[plugins]]` entries run in the order listed. `file` is a program or a `.js` file; a relative path counts from Razor's home. A program gets each event as JSON on standard input, and allows, changes or refuses it by what it prints and its exit code; a JavaScript file registers its own events and may add tools and commands. `razor plugins` lists what loaded. The full contract is in [docs/design/extensions.md](design/extensions.md).

```toml
[[plugins]]
file = "~/razor-hooks/check-calls"
before = ["call"]
```

| Setting | Default | What it does |
| --- | --- | --- |
| `file` | required | The plugin's file. |
| `before` | `[]` | The events a program sees before they happen, and may change or refuse: `prompt` (a conversation's system prompt as it opens), `input` (a message before it is recorded), `call` (a tool call before it runs), `rebuild` (a fresh context before it replaces the old). |
| `after` | `[]` | The kinds of record entry a program is shown once written, such as `input`, `text`, `call`, `result` and `turn_end`. |

### Always-on agents

`[residents.<name>]` defines an agent that runs as part of the server. Its keys are on [Always-on agents and schedules](always-on-agents.md#every-setting).

### Tasks

| Setting | Default | What it does |
| --- | --- | --- |
| `[tasks] remote` | none: plain files | A remote of the project's checkout, or a git URL, through which its tasks are shared, on the hidden reference `refs/razor/tasks` ([Memory](memory.md#tasks)). Only a project's `.razor/config.local.toml` may set it; any other file that sets it is refused. |

### The server

| Setting | Default | What it does |
| --- | --- | --- |
| `release_updates` | `true` | Download newer stable Razor releases at server startup and daily, ready for `razor restart`. Set `false` to stop future background checks; `razor update` still works. Source builds never update automatically, and with `provider_only` on none runs; `razor update`, typed by you, still does. |
| `[server] address` | none: the local socket only | A `host:port` the server also listens on, for other devices and for paired servers, the Razor servers on your other machines that you connect to this one; plain HTTP, so use a private network ([More machines](machines.md)). Needs a restart. |
| `[server] https_url` | none | An HTTPS address that reaches this server, used to finish MCP sign-ins. Without it, you paste the address the browser landed on. |
| `[server] name` | empty | This machine's short name: the first word of the name of each new session you start, and the machine name other servers show. Empty: sessions are named after their project. |
| `[server] label` | `name`, else the machine's name | How clients label this home. |
| `[server] realm` | empty | A label for this home, such as `work` or `personal`, that saved views can group sessions by. |
| `[server] shared` | empty: this home | The id (from `server.id` in its home) of the paired server that keeps the shared saved views and voice settings. |
| `[server] wrapper` | empty | A command the background service runs the server through, such as a script that sets up its environment: `razor install` puts it before the server's own command, which it receives as its last arguments and must run, for example with `exec "$@"`. Read when you run `razor install`. |

### The terminal app

| Setting | Default | What it does |
| --- | --- | --- |
| `[ui] motion` | `true` | `false` draws everything still, in its settled form. |
| `[ui] suggestions` | `false` | Suggest a next message in the composer, included in the agent's reply. This uses extra tokens but no extra request. Tab accepts it into the draft; send it separately. |
| `[ui] blocks` | `"auto"` | The workshop scene and the top bar's clock use octants, Unicode characters that split a cell into eight blocks, except in tmux or on narrow screens, where they use half blocks. `"octant"` also uses octants in tmux; `"half"` always uses half blocks. Narrow screens use half blocks for all three settings. Pictures in the transcript do not follow this setting ([Troubleshooting](troubleshooting.md#installing)). |
| `[ui] stage` | `"workshop"` | The status pane ends in a small animated workshop scene. `"off"` hides it and gives its rows to the pane. |
| `[ui] quit` | `"detach"` | What pressing Ctrl-C twice with an empty draft does: `"detach"` leaves the terminal and the session keeps running, as `/detach` does; `"end"` ends the session you are talking to at once, as `/end` does, then leaves. |
| `[brand] name` | `"Razor"` | The product name clients show. |
| `[brand] colours` | none | Replacements for the theme's colours, each name mapped to `"#rrggbb"`; the names are below. |

The theme's colour names:

```text
glass  bezel  bezelHi  recess  strip  data  white  gray  grayDim  cyan  green  magenta
amber  red  violet  violetDim  violetHi  lamp  agentBg  youBg  cyanBg  redBg  amberBg  addedBg
chip  button  group-slate  group-steel  group-indigo  group-plum  group-rose  group-clay
group-sand  group-moss  panel-slate  panel-steel  panel-indigo  panel-plum  panel-rose
panel-clay  panel-sand  panel-moss  panel  panelInk  panelGray  chipInk  clock  clockBg  askWords
```

```toml
[brand.colours]
violet = "#b48cff"
```

Saved arrangements of the agents list are not settings: `/views` edits them.
