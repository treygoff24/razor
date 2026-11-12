# More machines

Pairing connects the Razor servers on two machines, each knowing the other's key, so that the terminal app on either shows the sessions, questions and deliveries of both, and an agent on one can start a helper on the other or send it mail. Each server keeps its own sessions and records; pairing lets them reach each other.

Paired servers reach each other by key, over connections encrypted end to end between the two servers. They connect directly when the networks between them allow it and through a relay, which forwards packets it cannot read, when they do not. No VPN, address or open port is needed. Without a setting Razor uses the public relays run by iroh's developers; [Running your own relay](relay.md) says how to use yours.

## 1. Pair two servers

On one machine, name the other and show a code:

```sh
razor devices add laptop
```

It prints a code, as a QR code and as one line of text, and waits up to ten minutes. On the other machine, name the first and paste the line when asked:

```sh
razor devices join desk
```

The first command then says `laptop joined` and both servers list each other. The code works once; anyone who has it before it is used or expires can pair, so show it only where you would show a password. Ctrl-C withdraws it, and a second `razor devices add` replaces it.

A name is 1 to 32 lowercase letters, digits and hyphens. `razor devices` lists every paired server and enrolled program by name, and `razor devices remove <name>` removes one: it stops working at once.

Setup names this server from the system hostname and prints the name and where to change it: `name` under `[server]` in `~/.razor/config.toml`. The name also starts each new session's name; the display label follows it unless you set a separate `label`.

To use only relays you name, list them on each server:

```toml
[network]
relays = ["https://relay.example.net"]
```

Set it before pairing: a code carries the relays of the server that made it, and pairing refuses a server whose relays are outside the list. A relay you run yourself can admit only your people, each by a password in the address ([Running your own relay](relay.md)). `razor doctor` reports the relay connection and, for each paired server, whether it is reached directly or through a relay.

### Servers and apps paired by an earlier version

Earlier versions reached paired servers and the apps at an address on a private network, with a shared key on every request. Those pairs reach nothing now. Pair each one again once by code, under the name `razor devices` already lists for it: the entry is upgraded where it is, so a phone keeps its notification settings and the sessions it follows, and the old key and address are dropped. `[server] address` is no longer used, and `razor doctor` asks you to remove it. A `[remote]` that holds `address` and `key` is refused when you start a session; name the paired server instead ([below](#start-sessions-on-a-paired-server)).

## 2. The phone and Mac apps

The apps connect the same way: by key, encrypted end to end, with no VPN. On each computer the app should reach, name the device and show a code:

```sh
razor devices add phone
```

In the app, open Settings and choose Pair a Machine, then scan the QR code or paste the line. Pair the Mac app as `mac-app`. If `razor devices` already lists the phone or the Mac app from an earlier version, use that name, so it keeps its notification settings.

## What pairing gives you

| What | How it works |
| --- | --- |
| One list | Each machine's terminal app lists the other's sessions, questions and inbox beside its own, and opens, steers and stops them as if they were local. |
| Helpers on another machine | An agent can start a helper on a paired server by naming it; the helper works in that machine's checkout, and merging its branch happens there. |
| Mail across machines | Any agent can mail any other on a paired server, by name or session id. |
| One nightly pass | For a project whose memory is shared, one of your paired servers runs the night's pass and reads your sessions from all of them ([Memory](memory.md#several-machines)). |
| Shared settings | One server can keep the saved arrangements of the agents list (`/views`) and the voice settings for all of them: set `[server] shared` on the others to its id, the contents of `server.id` in its Razor home. |

A server that stops answering keeps its last rows in the list, marked with how long since it was heard from. A Mac's server says it is going to sleep a second before the Mac sleeps, so the servers paired with it mark it at once instead of when their reads time out, and they list it answering again when it wakes. A paired server's key is a full key: it can do on this server anything this server's own terminal can, devices, settings and sign-ins included, so pair only machines you trust as you trust this one. Each pairing is direct: a server shows the servers it has paired, not their pairs.

## Start sessions on a paired server

Pair the two servers by code, then name the pair in the terminal machine's `~/.razor/config.toml`:

```toml
[remote]
server = "desk"
# folder = "/home/me/project"  # Optional path on desk.
```

Plain `razor`, `--continue`, `--resume` and `--print` use the local server's forwarding
routes to reach `desk`. New sessions and `--continue` use `folder` when set, otherwise
the path of the directory where you launched Razor; that path must exist on the
paired server. An unqualified `--resume` id addresses `desk`; an id already written
as `<id>@<server>` keeps its server. The new-agent chooser can select another folder
or server.
Configure `play` and `capture` on the terminal machine so its speakers and microphone
are used; the session's owner supplies speech and transcription.

## More than one Razor on one machine

Each Razor home runs its own server, so one machine can hold two separate Razors, each with its own settings, keys, ChatGPT sign-in and sessions (Claude's sign-in belongs to your user account, so they share it), by pointing `RAZOR_HOME` at a second folder:

```sh
RAZOR_HOME=~/.razor-second razor
```

Each home has its own network key, so other machines pair with each one separately. `razor install` registers each home's background service under its own name, the home's folder without its leading dot (`razor` for `~/.razor`, `razor-second` for `~/.razor-second`), so each home can run as a service; it never replaces a service of the same name that runs another home.
