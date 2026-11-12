# Troubleshooting

Start with `razor doctor`. It starts no server, Razor writes nothing while it runs, and it prints no key's value; the Claude sign-in check it runs is Claude Code's own, which may create `~/.claude` and `~/.claude.json` if they are missing. It prints one line for each thing Razor runs on, found or missing with what to do, then the models it can run on.

```sh
razor doctor
```

```text
ChatGPT sign-in: missing. ChatGPT is not signed in; run razor login.
Claude: found.
EXA_API_KEY: found.
FIRECRAWL_API_KEY: missing. For optional web search, run razor keys.
PARALLEL_API_KEY: missing. For optional web search, run razor keys.
ELEVENLABS_API_KEY: missing. For optional voice, run razor keys.
Browser: found (agent-browser executable; Chrome launch untested).
Desktop: found (cua-driver daemon answering; operation untested).
Server: running.
Service: installed.
Razor can run on Claude.
```

It exits 0 when your settings load and Claude or GPT can run. A missing ChatGPT sign-in or API key does not matter if you use Claude, and the reverse; missing web keys only mean agents use their model's own web tools, and a missing voice key that nothing is read aloud. The OpenAI API key line matters only if you bill GPT through an API key instead of a ChatGPT plan. `Server: stopped; your next razor command starts it.` means no server is running for this home, which is normal before your first `razor` command; if a command then fails to start one, see [the server and settings](#the-server-and-settings) below. On a Mac with the server running, a `Mac permissions` line also says whether agents can see the screen and use the keyboard and mouse; when the service runs the server and either is missing, doctor asks whether to request them from macOS, and yes puts Razor in both lists in System Settings ([Security](security.md#screen-and-control-on-macos)).

Two places hold more detail. The server writes its errors to `~/.razor/server.log`; when it runs as the service that `razor install` sets up (the same server, started by systemd or launchd), read them on Linux with `journalctl --user -u razor`. The service is named after its home's folder without the leading dot, so a `RAZOR_HOME` of `~/.razor-work` has `razor-work`. Each session's folder, `~/.razor/sessions/<id>/`, holds its record and `startup.md`, which shows exactly what the conversation was sent at its start (`/startup` shows it in the terminal app).

## Restarting the server

Some changes apply only when the server starts, such as new keys in `keys.toml`. Restart this home's server with:

```sh
razor restart
```

It waits for the old server to stop, then starts the installed background service, or starts a detached server if you have no service. To stop without starting again:

```sh
razor stop
```

Stop succeeds if no server is running. A later `razor` command, `razor --print` and the `razor mcp`, `login` and `devices` commands start it again: the installed service if there is one, otherwise a server in the background. Set `RAZOR_HOME` on either command to select a different home. Commands the agents started keep running through a stop, so that the next server can take them back.

Sessions that were working when the server stopped resume by themselves if their last entry is less than an hour old. Older ones show as interrupted; send them a message, or type `/continue all` to continue every interrupted top-level session.

## Installing

**The workshop scene or the clock looks blocky inside tmux.** Razor draws them with half blocks there by default because some tmux width tables cannot handle octant glyphs. If your terminal, font and tmux support them, run `razor config set ui.blocks octant`. Narrow screens still use half blocks. If the glyphs look broken, use `razor config set ui.blocks half` or restore the default with `razor config unset ui.blocks`.

**Pictures in the transcript are coloured blocks, not sharp.** The terminal app draws a picture's own pixels only in a terminal that answers the Kitty graphics protocol, such as Kitty, Ghostty or a recent iTerm2. Other terminals, and every terminal reached through tmux or mosh, get coloured half blocks. A picture stays a one-line chip when it cannot be read, is too large, is in a format the terminal app does not draw (such as an animated or lossless WebP), or has too little room on screen.

**`razor: command not found` after installing.** The installer links `razor` into `~/.local/bin` and, when that folder is not on your `PATH`, prints the command that adds it. For sh, bash and zsh, add this to your shell's profile; for fish, run `fish_add_path "$HOME/.local/bin"`:

```sh
export PATH="$HOME/.local/bin:$PATH"
```

**`Unsupported platform; use Linux x86_64 or aarch64, or macOS arm64.`** Releases and the install script support only those three platforms.

**`The Claude bridge at … has no node_modules; run npm ci there.`** A build from source is missing the packages of its bridge, the small Node program Claude sessions run through. Run `npm ci` in the `bridge` folder of your checkout, or install with `scripts/install.sh --link`.

**`Run razor setup in a terminal.`** Setup, and `razor keys`, ask their questions in a terminal. For a script, sign in with `razor login` for ChatGPT, and if you use only ChatGPT put `model = "sol"` in `~/.razor/config.toml`; for Claude, sign in with Claude Code itself if you have it, or put `ANTHROPIC_API_KEY` in `~/.razor/keys.toml`; for any other provider's key, put its variable there and name its model in `model` ([Providers](models.md#providers)). `razor install` sets up the background service without asking, and `~/.local/share/razor/current/agent-browser install` downloads the Chrome the browser drives. Then run `razor doctor`. Optional keys can go straight into `~/.razor/keys.toml` ([its format](configuration.md#keys)).

**`Release checksum mismatch; download again from the release URL.`** or **`Node checksum mismatch; download again from nodejs.org.`** A download was cut short or changed on the way, and the installer stopped before installing anything. Run the one-line installer again; each run downloads afresh. An error starting `curl:` instead, such as `curl: (22) The requested URL returned error: 404`, means the download itself failed: check your connection and the address in the command.

## Signing in

**`Razor cannot run on its default model, …, yet`** Do what the `Model:` line above it says: on Claude's or ChatGPT's sign-in, what its `Claude:` or `ChatGPT sign-in:` line says, such as `razor setup` for Claude (`razor setup --console` for an Anthropic Console account), `razor login` for ChatGPT, or installing Node; on a provider, save its key, or fix the address, format or model id it names under `[providers]`. Then run `razor doctor` again. To use another model, run `razor setup` or set `model` in your settings.

**`ChatGPT is not signed in; run razor login.`** Run `razor login`, open the link it prints on any device, and enter the code it shows.

**`Claude is not signed in; run razor setup.`** Claude's sign-in is missing or expired. `razor setup` runs it again, or `razor setup --console` for an Anthropic Console account. On a machine with no browser, open the link it prints on any device and paste the code back.

**`The Claude bridge needs Node 22.18 or later on PATH (…); this install has no private Node; install Node, then run razor doctor.`** Claude sessions run through a small Node program. The installer downloads a private copy of Node only when it finds no Node 22.18 or later on your `PATH`; when it found yours, Razor keeps using the `node` on the `PATH` of whatever starts the server, and the line shows that `PATH`. Put Node 22.18 or later on it, run `razor install` again if the background service needs the new `PATH`, then `razor restart`.

**`… is not set; save it with razor setup or in keys.toml, then run razor restart.`** or **`The model host refused … or never got it; …`** A provider's model needs a key variable the server does not have, or the host refused the key it got. The first sentence is the model picker's reason for not offering the model; the second ends a turn on it, such as one from `razor --print --model gemini`. Save the key in `~/.razor/keys.toml` ([Keys](models.md#keys)), or export it where the server starts, then run `razor restart`.

**`provider_only is on, so Razor …`** The setting that keeps Razor to your model providers refused something; the sentence says what. Use a model on a provider under `[providers]`, or turn the setting off with `razor config set provider_only false` ([Security](security.md#contacting-only-your-model-provider)).

## The server and settings

**`Settings: invalid. <file> does not parse: …`** from `razor doctor`. A settings file has a mistake, named with the file and the problem; the server will not start until it loads, so doctor exits 1. Fix that file and run `razor doctor` again.

**`… keys.toml holds lines NAME = "key", each name letters, digits and _, each key a nonempty string.`** `keys.toml` holds a line that is not a variable's name and its key, and neither the server nor doctor starts until it is fixed ([Keys](configuration.md#keys)).

**`razor serve exited (exit status: 1) before answering; see …/server.log; last line: "…"`** The server a command started stopped at once, and the quoted line from `~/.razor/server.log` says why. The usual reason is a settings file that does not parse, named with the problem: fix the file and run the command again.

**`No Razor server answered within 10 seconds; … says why.`** A server started but did not answer in time. The message names where its errors are: `~/.razor/server.log`, or, when the background service started it on Linux, `journalctl --user -u razor.service`, whose name for another home is that home's folder without its leading dot. Read the last lines there.

**`Razor cannot listen on …/server.sock: …`**, usually as the last line of a `razor serve exited` message. The server could not make its socket in Razor's home, and the error after the path says why. `Operation not permitted` means something outside Razor forbids a socket there, such as a sandbox that an agent tool or a container runs commands in: run Razor outside it, or set `RAZOR_HOME` to a folder where it may make one. `path must be shorter than SUN_LEN` means the socket's full path is too long, which is over 107 bytes on Linux and 103 on macOS: set `RAZOR_HOME` to a shorter path.

**`A Razor server is already running for …`.** `razor serve` found another server using the same home, often the service and a server the terminal app started in the background, both trying. Run `razor restart` to stop this home's server and start it again. To install the background service, run `razor install`; it stops the old server first.

**`… cannot set …; move these keys to the person's file.`** A project's `.razor/config.toml` holds a setting only your own file may set ([Configuration](configuration.md#what-only-your-own-file-may-set)). Move it to `~/.razor/config.toml`.

**`This session has … working children and children.max is …`.** A session reached its limit of working helpers. Wait for one to finish, or raise `[children] max`; it applies at once.

**A session shows as stuck.** It has had unanswered input for 15 minutes with no command running, no question open and no working helper. Open it to see what it last did; Escape stops its turn, and a new message gives it something to do.

## More machines

**`Settings: server.address is no longer used`** (from `razor doctor`). Servers and apps no longer reach each other at an address. Delete `address` under `[server]` in the file doctor names; nothing else changes.

**`[remote] names a paired server, as server = "<its name in razor devices>", and holds no address or key`.** Your `[remote]` was written for an earlier version, which reached the other server at an address with a key. Pair the two servers by code ([More machines](machines.md#1-pair-two-servers)), then replace `address` and `key` under `[remote]` with `server = "<the name razor devices lists for it>"`.

**`Pair <name>: unreachable; check that server and its relay.`** (from `razor doctor`). This server could not reach the paired one. Check that its server is running and that `razor doctor` there shows its home relay connected. A server or app paired by an earlier version reaches nothing until it is paired again by code ([More machines](machines.md#servers-and-apps-paired-by-an-earlier-version)). A pair that answers shows `direct` or `relayed through` a relay, with the round trip.

**`Home relay: … (refused this server; …)`** (from `razor doctor`). The relay turned this server away, usually because the password in its address under `[network] relays` is wrong or missing ([Running your own relay](relay.md)). `disconnected` or `not reached` instead means the relay did not answer: check its address and that this machine can open a WebSocket to it.

## Voice, notifications and the browser

**`speech: there is no ElevenLabs key; run razor keys to add one.`** Reading aloud and agents' spoken notes need `ELEVENLABS_API_KEY`. Run `razor keys` (Enter keeps the keys you already saved), then `razor restart`.

**`speech: Python (…) has no working SSL support`** (or `unknown url type: https` from an older speech program). The Python named in the error cannot load its SSL module. The server's `PATH` may select a version-manager Python whose OpenSSL library was removed. Find a working interpreter with `/path/to/python3 -c 'import ssl'`, then set these absolute paths at the top level of `~/.razor/config.toml`, before any `[section]`:

```toml
speech = ["/path/to/python3", "/absolute/path/to/scripts/speech.py"]
```

For an installed copy, the script is `~/.local/share/razor/current/scripts/speech.py`; expand `~` to your home directory in the setting. The setting is live, so try ▶ again after saving it.

**`speech: The API key you used is missing the permission … The key needs Text to Speech and Voices (read).`** Your ElevenLabs key is restricted and lacks a permission speech uses. In ElevenLabs' API key settings, give it Text to Speech and Voices (read), or make it unrestricted; no restart is needed.

**`Over SSH, audio would play on <machine>, not your machine; pair Razor on yours.`** The terminal app plays audio on its own machine, which over SSH is the remote one, so nothing plays; recording over SSH is refused for the same reason. Install Razor on the machine you sit at and [connect its terminal to the other server](machines.md#start-sessions-on-a-paired-server); audio then uses your own speakers and microphone ([Replies read aloud](browser-voice-search.md#replies-read-aloud)).

**No desktop notifications.** On macOS, allow notifications for Script Editor in System Settings, under Notifications. On Linux, the starter pack (the default settings Razor ships) uses `notify-send`; on a machine with no desktop there is nowhere to show them, so send them to your phone with ntfy ([Configuration](configuration.md#notifications)).

**`Browser: missing or unresponsive. Reinstall the agent-browser executable, then run agent-browser install.`** from `razor doctor`. The agent-browser program beside Razor, `~/.local/share/razor/current/agent-browser`, is missing or does not run. When it is missing, Razor uses the `agent-browser` on your `PATH`, so `npm install --global agent-browser`, then `agent-browser install` to download its Chrome, brings it back ([Browser, voice and search](browser-voice-search.md#the-browser)).

**`Desktop: not running.`** from `razor doctor`. Agents operate desktop apps through Cua Driver's background program, which the server runs only on a Mac where Razor has Accessibility, or on Linux when the server was started with a display (`DISPLAY` set); on a headless server this line is expected. On a Mac, let doctor request the permissions, turn Razor on in System Settings, then run `razor restart`. If it stopped on its own, `~/.razor/server.log` holds its last words and the line `Desktop control is off: …`; fix what they name, then run `razor restart` ([Browser, voice and search](browser-voice-search.md#desktop-apps)).

**The browser will not start (Linux).** Chrome lacks system libraries. Run `~/.local/share/razor/current/agent-browser install --with-deps`, which uses `sudo`. `~/.local/share/razor/current/agent-browser doctor --offline` says what else is wrong, with a test launch.

## Updating

Run `razor update`, then `razor restart` to use the downloaded version. Background updates leave the running server on its existing version. The terminal's restart notice appears once per launch; closing and reopening the terminal does not restart the server. Old bundles stay available while a process runs from them.

If an update fails, the installer reports the cause and keeps the selected version. Check your connection and run `razor update` again. Background failures go to the [server log](#restarting-the-server). To stop future automatic checks, run `razor config set release_updates false`; an update already running finishes.

**`This is a source installation`.** Builds run from a checkout, and bundles installed with `scripts/install.sh`, update from that checkout. They never download releases automatically. To switch to a published release, run the install command in the [Quickstart](quickstart.md).

**`Razor has no command update`.** v0.1.0 predates this command. Run the release install command once more, then `razor restart`. Future releases can update themselves.
