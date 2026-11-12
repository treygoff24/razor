# Security

Razor's agents run commands as you, with your files, your network and your credentials. Razor does not sandbox them. It keeps its own keys out of what agents see by accident, refuses two kinds of dangerous command, and on the network answers only the devices you paired. Decide what you let an agent do the way you would for a person you give a shell to.

## What the server exposes

The server listens on a Unix socket in its home, `~/.razor/server.sock`, and every request there must carry a key: the local key in `~/.razor/device.key`, readable only by you, or the key of a program you enrolled with `razor devices add <name> --key`. `GET /v1/mcp/callback` is exempt, where the browser returns from an MCP server's sign-in and cannot carry a key; it finishes only a sign-in the server is waiting on, matched by that sign-in's secret `state` value. Every other operation requires a key.

Paired servers and the phone and Mac apps reach it through its network endpoint, over connections encrypted end to end and authenticated by each device's own key ([More machines](machines.md)). A connection from a key that is not enrolled is closed before it reaches Razor's API, and no key or password crosses the network after pairing. Pairing answers a stranger only while a code from `razor devices add` is outstanding, and only with that code's secret; the code works once and expires after ten minutes. Nothing on the network speaks plain HTTP to Razor. The endpoint also keeps a connection to a relay, iroh's public ones unless you name your own ([Running your own relay](relay.md)); a relay sees which keys connect, from where and how much they send, never what. With `provider_only` the server opens no network endpoint at all.

A paired device can do everything the local terminal can, and a paired server is a paired device: a server you pair can do on this one anything its terminal can, devices, settings and sign-ins included. Pair only machines you trust as you trust this one. What a server passes along for you is narrower: operations on a session that lives on the other server, and listing or starting sessions and listing models and repositories there, each forwarded once and never relayed onward. `razor devices remove <name>` removes a device at once and closes its connections.

## Where keys live

| What | Where |
| --- | --- |
| API keys you save with Razor: providers' from setup, web and voice from `razor keys` | `~/.razor/keys.toml`, mode 0600 |
| ChatGPT sign-in | `~/.razor/auth.json`, mode 0600 |
| Claude sign-in | Claude Code's own store: `~/.claude`, or the login keychain on macOS |
| MCP servers' sign-in tokens | `~/.razor/mcp-auth.json`, mode 0600 |
| The local key, enrolled devices and paired servers | `~/.razor/device.key` and `~/.razor/devices.json`, mode 0600 |
| This server's network key | `~/.razor/node.key`, mode 0600 |
| Keys you export instead | The server's environment, under the names their providers give |

No setting needs a key's value: a provider names the variable that holds its key, and an MCP server's headers and client secret are `{ env = … }` or `{ file = … }`. The one exception is an MCP program's `env` table, which is passed to the program as written, so a value put there sits in the file. `razor doctor` says which sign-ins and web keys are present and never prints one.

## What agents can run

An agent's shell is bash, as your user, in the session's folder. It starts from the server's environment minus the web and voice keys, every name `keys.toml` lists and Claude's sign-in variables (`EXA_API_KEY`, `PARALLEL_API_KEY`, `FIRECRAWL_API_KEY`, `ELEVENLABS_API_KEY`, `CLAUDE_CODE_OAUTH_TOKEN`, `ANTHROPIC_API_KEY` and `ANTHROPIC_AUTH_TOKEN`), so a command that prints its environment does not put them in the record. MCP servers and script tools start the same way. That is protection against accidents, not against an agent that sets out to read a file: anything your user can read, an agent can read, `keys.toml` included. Keys you export for other models and do not list in `keys.toml`, such as `GEMINI_API_KEY`, stay in agents' environments.

Razor refuses two kinds of command, by checking the command's text before it runs:

| Refused | What counts | Can it be turned off |
| --- | --- | --- |
| A kill by pattern | `pkill`, `killall`, `kill -1`, or `kill` fed by `pgrep` or `pidof`. A plain `pkill` whose pattern matches the session's own running jobs stops those jobs instead. | No |
| Deleting what the session did not create | `rm`, `rmdir`, `unlink`, `shred`, `find -delete`, `find -exec rm`, `git clean` without `-n`, or a patch that deletes a file, aimed at a file that existed before the session started or whose creation time the filesystem does not record, or at a path written with a wildcard, `~`, a variable or a command substitution | Yes: `[guards] delete = false` in your own file or a pack (a folder of default settings, such as the starter pack Razor ships) |

Both are best effort. The check sees through `sudo`, `env`, `nohup`, `timeout` and similar wrappers, but a program an agent runs can still kill or delete whatever you could, a command can be written in ways the check does not see, and script tools' commands are not checked. Plugins can refuse more under their own rules ([Configuration](configuration.md#plugins)).

## Contacting only your model provider

`provider_only = true` in your own file ([Configuration](configuration.md#providers)), which setup's "Contact nothing but your model provider?" sets, is for a work machine whose only model access is, say, a company's AI gateway: Razor itself then reaches only the providers named under `[providers]` in your settings, and refuses the rest with a sentence naming the setting.

- A session runs only on a model of a provider with a `[providers]` table; Claude's and ChatGPT's own sign-ins never count. A default model on no such provider is a settings error, which `razor doctor` and the server's start report.
- Razor opens no network endpoint, so it neither pairs with other Razor servers or apps nor connects to those it is already paired with.
- No release or skill updates are downloaded unless you run `razor update` yourself, and setup does not download the browser.
- Razor's own web tools (`search`, `fetch`, `answer`), Claude Code's WebFetch, which fetches pages from this machine, the speech and transcription programs, the phone's live calls and the video watcher are off. A model's own web search, which runs inside its provider, stays.
- MCP servers you add yourself, in your own file or a project's private `.razor/config.local.toml`, still start; those a project's shared `.razor/config.toml` brings do not, and `razor mcp list` says how to add one yourself.
- Claude Code, on a provider's Anthropic-format endpoint, is asked to send nothing to Anthropic beyond that endpoint.

It covers what Razor does by itself. What you configured yourself stays on, and where it reaches is yours to judge: MCP servers you added, the `notify` and `ask` programs, `[push]` to Apple's push service, a project's `[tasks] remote`, the Post watch when Post is installed, plugins and script tools, and the commands agents run, since a shell command can reach anything this machine can and only a firewall governs that. Razor's Cua Driver program for desktop apps stays on too, contacting nothing on its own: Razor runs it, and doctor's check of it, with `CUA_DRIVER_RS_TELEMETRY_ENABLED=false` and `CUA_DRIVER_RS_UPDATE_CHECK=false`, which turn off its usage reporting and its update check, and agents' shells get the same two settings.

## Screen and control on macOS

The shipped Cua Driver program (`cua-driver`) can read and operate desktop apps through accessibility and take screenshots; the server runs its background program as its own child, so it acts with Razor's permissions, and only once Razor has Accessibility. Turning on Razor under Accessibility (Device Control and Data Access on macOS 27) and Screen & System Audio Recording lets every agent on that Mac, in every Razor home, see the screen and control the computer: read any window, type, click and answer dialogs as you. macOS gives the permission to Razor.app, the program the background service starts first, not to one agent or one home, and `razor install` keeps Razor.app at one path so that the permission can outlast updates. Leave both off unless you want agents to do that. `razor doctor` says whether the running server has them and, in a terminal, offers to ask macOS for them, which only puts Razor in both lists; nothing is on until you turn it on, and turning Razor off in System Settings takes them back.

## Opening an untrusted repository

Opening a session in a repository starts the MCP servers its settings name, without asking (with `provider_only` on, not those of its shared `.razor/config.toml`), so only open sessions in repositories you trust, as you would only run their build scripts if you trusted them. A repository's Razor settings are `.razor/config.toml` and `.razor/config.local.toml` at its root.

What a repository can do when you open a session in it:

- start MCP servers: programs, named in its settings, that run as you as soon as the session opens;
- add script tools, commands an agent can choose to call;
- steer the agent through its `AGENTS.md` and `CLAUDE.md` files, at the root and in each folder down to the session's, which go into every conversation's opening;
- add skills, in its `.agents/skills` and `.claude/skills` folders, which every conversation's opening lists;
- change project-level settings such as the model, the web providers and the helper limits.

What it cannot do: set anything about you or the machine, such as the server, plugins, guards or the notification programs ([the full list](configuration.md#what-only-your-own-file-may-set)).

To look before you open one, read its settings and instructions, and list its skills:

```sh
cat .razor/config.toml .razor/config.local.toml 2>/dev/null
find . -name AGENTS.md -o -name CLAUDE.md
ls .agents/skills .claude/skills 2>/dev/null
```

## What is kept on disk

Every session's record, `~/.razor/sessions/<id>/record.jsonl`, holds the whole conversation in plain text: your messages, the agents' replies, every command and its output. Records and memory files are written with your account's default file permissions, which on many systems let other users read them, so Razor makes its home readable only by you (mode 0700) when it creates it. A home that already existed keeps its mode; on a machine other people can log in to, check it with `ls -ld ~/.razor` and make it private with:

```sh
chmod 700 ~/.razor
```

When an agent types a password into an interactive program, it can send it from a file so that the password never enters the record; what the program prints back is recorded as usual.
