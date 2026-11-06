---
name: razor-settings
description: Razor itself. Explain how it works, change how it is set up for the person (its settings files, keys, the about file, MCP servers, notifications to a phone, residents and their schedules), and diagnose it with razor doctor and its installed docs.
---

# Razor's settings

## Razor's own docs

The docs for the installed version are in the `docs` folder beside the `razor` program. Razor puts its own install folder first on your shell's `PATH`, so their index is `"$(dirname "$(command -v razor)")/docs/README.md"`, which in a standard install is `~/.local/share/razor/current/docs/README.md`. Read the page for the question before answering from memory: `configuration.md` lists every setting with its default, `troubleshooting.md` the common failures, and `scripting.md` `razor --print` and the local API; `CHANGELOG.md`, beside the `docs` folder, says what each release changed. A build from source has no such folder; read the checkout's `docs/` instead, or github.com/treygoff24/razor/tree/main/docs, which may describe a newer version than the one installed.

`razor doctor` says what is signed in, which keys are present, whether the settings load, whether the browser and desktop-control programs, the server and the background service are there, and which models Razor can run on, with the command that fixes each problem. It starts no server, Razor writes nothing while it runs, and it never prints a key. Run it first when something does not work. `razor --help` lists every command, and `razor help <command>` shows one.

## Change a setting

Use `razor config set ui.blocks half` for a simple change. See `configuration.md` for the commands, settings, file scopes and restrictions; `/keys` shows effective keyboard defaults.

## Where settings live

Razor reads up to four files, each later one winning key by key:

1. The pack's `config.toml`. On a new install that is the starter pack, `packs/starter/config.toml` in Razor's install folder. Its commented lines are worked examples of the common optional settings: read it before writing one from memory. Never edit it; an upgrade replaces it.
2. The person's own file, `$RAZOR_HOME/config.toml` (`~/.razor/config.toml` unless `RAZOR_HOME` says otherwise). Change their settings here.
3. A project's `.razor/config.toml` at the repository's root, shared with everyone who works in it.
4. That project's `.razor/config.local.toml`, private to this machine.

A table in two files merges key by key; a named entry such as `[mcp.files]` comes whole from the file that wins, and a list is replaced, never joined. A relative path counts from the file that names it, so to change the pack's `startup` list, copy the whole list into the person's file and copy the pack's files it names (`persona.md`, `rules.md`) beside that file. Settings about the person or the machine are refused in a project's files; `configuration.md` lists them under "What only your own file may set".

Razor rereads a file when it is saved, and most changes apply at the next turn; `configuration.md` names the few that need a restart. A file that does not parse is refused whole and its last good settings stay. `razor restart` stops this home's server, and with it every working agent's turn, this one's included, so ask the person before running it; the turns resume when the server is back.

## Terminal keys

The terminal app's keys are settings like any other, in the person's own `config.toml`: one table per context, `[ui.keys.<context>]`, each action a list of keys. Set only the actions the person wants changed; the rest keep their defaults, and `[]` unbinds one. The terminal applies a saved change at once, with no restart.

To find an action's name, have the person run `/keys` in the terminal (or press the prefix, backtick or Ctrl-B with an empty draft, then `?`): it lists every action as `context.action: keys · what it does`, and `/` searches it. `configuration.md` under "Terminal keys" lists the contexts and every prefix action by name; all the shipped defaults are in Razor's `src/terminal/data/keys.toml`, in the same format.

```toml
[ui.keys.global]
prefix = ["Ctrl-A"]          # instead of backtick and Ctrl-B

[ui.keys.prefix]
detach = ["q", "d", "D"]
```

A `[ui.keys.prefix]` entry may also name any action in full, quoted, which then runs after the prefix as its own key does; free its own key with `[]` in its context:

```toml
[ui.keys.prefix]
"global.status" = ["t"]      # prefix then t opens the status pane

[ui.keys.global]
status = []                  # and Ctrl-T no longer does
```

A key is `Ctrl-`, `Alt-`, `Shift-` or `Super-` before a named key (`Enter`, `Esc`, `Tab`, `Left`, `PageUp`, `F2`) or one character; a shifted letter is written as its capital (`"T"`) and a shifted digit as its symbol (`"!"`). An unknown action, a malformed key or one key given to two actions in the same context is refused: the terminal shows the error and keeps its last good keys, so check each name and key before saving.

## Keys

API keys never go in a settings file. `razor keys` stores them in `$RAZOR_HOME/keys.toml`, readable only by the person, and Razor reads that file when its server starts, so run `razor restart` after a change. When a requested web or voice capability needs a missing key, point the person to `razor keys`, then `razor restart`; `razor keys` needs their terminal, so it is theirs to run. A model's key, such as `GEMINI_API_KEY`, is not one of these: it goes in the server's environment (`models.md`). Never print a key, and never copy one into a file or a message.

## About the person

`$RAZOR_HOME/about.md` is read into every conversation: a first line `# About <name>`, then a few lines in the person's words. Create or change it only when the person asks, using their words. Keep it short.

## MCP servers

In the shell: `razor mcp list`; `razor mcp add <name> --scope global|project|private --config '<JSON>'`; `razor mcp remove|enable|disable <name> --scope <scope>`; `razor mcp reconnect|sign-in|sign-out <name>`. Without `--scope`, a change goes to the project's private file. `razor mcp import <file>` shows which servers a Claude Code, Codex or OpenCode settings file holds without writing anything; the person imports them with `/mcp import <file>` in the terminal app. A server that needs a sign-in sends the person a link; when the browser lands on an address Razor cannot receive, `razor mcp complete '<that address>'` finishes it. Changes apply at the next turn.

## Skills

A skill is a folder holding a `SKILL.md`, loaded by name with the `skill` tool. Razor looks first in its own updated copies of pack skills (`$RAZOR_HOME/skills`), then in the pack's `skills` folder, then the project's `.agents/skills` and `.claude/skills`, then the person's own `~/.agents/skills`, `~/.claude/skills` and `~/.codex/skills`, so the skills they use in Codex and Claude Code carry over, and last in the folders the `skills` key names; the first folder holding a name wins. A new conversation's opening lists all but the `skills` key's folders with their descriptions, up to 100, so a skill added now is listed from the next new conversation and loads by name at once. A new skill for the person goes in `~/.agents/skills/<name>/SKILL.md`, with a `description:` line in its front matter.

Unless `skill_updates = false`, Razor updates installed skills daily: a clean Git checkout of skills is fast-forwarded, a skill the `skills` CLI installed is reinstalled by it, which can overwrite local edits, and a pack skill that follows an upstream branch gets a newer copy in `$RAZOR_HOME/skills`. To tailor an installed skill for the person, copy it to a new name in `~/.agents/skills` and edit the copy.

## Notifications

Notes and questions from agents arrive as desktop notifications through `notify` and `ask`. To reach a phone instead, the starter pack's ntfy lines go into the person's file with a topic only they know. A program named in `notify` gets the text as its last argument; one named in `ask` may answer by printing the answer and exiting 0, so it must print nothing else.

## Residents

A resident is an agent that runs as part of the server in its own folder, with its own session, waking at set times. `always-on-agents.md` has every setting; a schedule looks like this:

```toml
[residents.scout]
cwd = "~/scout"
[residents.scout.schedules.weekly]
at = ["08:00"]
days = ["mon"]
text = "Summarize last week's changes in this folder."
deliver = "notify"   # or "ask", or leave it out
fresh = true         # run in a new session of its own each time
```

Residents run only while Razor's server does; `razor install` installs the server as a service (systemd on Linux, launchd on macOS).
