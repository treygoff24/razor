# More machines

Pairing connects the Razor servers on two machines, each holding a key for the other, so that the terminal app on either shows the sessions, questions and deliveries of both, and an agent on one can start a helper on the other or send it mail. Each server keeps its own sessions and records; pairing lets them reach each other.

Paired servers talk plain HTTP with a bearer key on each request, so they must reach each other over a private network that encrypts traffic and decides who can connect. [Tailscale](https://tailscale.com) does both and is what this page uses.

## 1. Put the machines on Tailscale

Install Tailscale on each machine and sign in to the same Tailscale network. Then run on each:

```sh
razor setup
```

If the machines belong to different Tailscale networks (tailnets), use [Tailscale node sharing](https://tailscale.com/kb/1084/sharing): a device connects to one tailnet at a time. Share each machine with the user on the other tailnet and accept both invitations, so both Razor servers can start connections to each other. A share in only one direction allows connections to the shared machine, but does not let it start connections back. Check that both tailnets' access rules allow the chosen Razor port before pairing.

Setup names this server from the system hostname and prints the name and where to change it: `name` under `[server]` in `~/.razor/config.toml`. The name also starts each new session's name; the display label follows it unless you set a separate `label`.


With Tailscale running, setup fills an unset `[server] address` with this machine's Tailscale name (or its IP address if the name does not resolve here) and the first available port from 7433 through 7442. Your existing settings survive a rerun. Run `razor restart` if the server was already running when setup added its address. If Tailscale starts later, Razor keeps serving its local terminal and tries the configured address again every five seconds. `razor doctor` reports when it is not listening.

## 2. Find and pair the other server

Run on either machine:

```sh
razor devices find
```

It looks for online Razor servers on your tailnet. Suggested names come from Tailscale; the name each server reports appears beside them. Plain `razor devices` lists enrolled devices without scanning. Each discovered server comes with its add command, for example:

```sh
razor devices add laptop --address 100.104.105.106:7433
```

This enrolls the laptop on this machine and sends it a pairing request. Razor prints the confirmation command to run on the laptop:

```sh
razor devices add desk --address 100.101.102.103:7433
```

Run it there within ten minutes. That server verifies that the request's key belongs to its own enrollment on the server at the address you chose and enrolls this machine with the same key. Both sides require your add command; finding a machine never grants it access. If several machines have the same name, choose a different name in your add command. After a request expires or a server restarts, remove the incomplete enrollment and start again.

If the other server cannot receive the request, Razor prints a command with `--key` instead; run that command on the other machine. Treat the key like a password. Older Razor versions use this manual path too.

## Manual addresses and discovery limits

Without Tailscale, use another private network that encrypts traffic, and set each server's reachable address yourself:

```toml
[server]
address = "100.101.102.103:7433"
name = "desk"
```

Then run `razor restart` and `razor devices add laptop --address <laptop-address:port>`. Bind to the private network's address, never `0.0.0.0` or a public address: anyone who can reach the port can try keys against it. With Tailscale you can find the IP using `tailscale ip -4`.

Discovery runs only when you call `razor devices find`. It checks ports 7433 through 7442 and this server's configured port on up to 256 online peers, with 64 requests at once, replies up to 1 KiB, and a five-second scan limit after reading Tailscale's status. Large tailnets may return partial results. Custom ports outside that range on other servers need a typed address unless they match this server's port. Tailscale access rules and host firewalls must allow the connection.

A name is 1 to 32 lowercase letters, digits and hyphens. `razor devices` lists every paired server and enrolled program by name, and `razor devices remove <name>` removes one: its key stops working at once. Without `--address`, `razor devices add <name>` enrolls a client program rather than a server and shows its key once; treat that key like a password, since it can do anything the local terminal can.

## What pairing gives you

| What | How it works |
| --- | --- |
| One list | Each machine's terminal app lists the other's sessions, questions and inbox beside its own, and opens, steers and stops them as if they were local. |
| Helpers on another machine | An agent can start a helper on a paired server by naming it; the helper works in that machine's checkout, and merging its branch happens there. |
| Mail across machines | Any agent can mail any other on a paired server, by name or session id. |
| Shared settings | One server can keep the saved arrangements of the agents list (`/views`) and the voice settings for all of them: set `[server] shared` on the others to its id, the contents of `server.id` in its Razor home. |

A server that stops answering keeps its last rows in the list, marked with how long since it was heard from. A paired server's key is a full key: it can do on this server anything this server's own terminal can, devices, settings and sign-ins included, so pair only machines you trust as you trust this one. Each pairing is direct: a server shows the servers it has paired, not their pairs.

## Use a terminal without a local server

To run the terminal on the machine where you sit while its sessions live on another machine, install Razor and Tailscale on both. Give the server a private Tailscale address as above, then run `razor devices add laptop` on the server. Copy the key it prints securely to a private file on the laptop, for example `~/.razor/desk.key` (readable only by you). In the laptop's `~/.razor/config.toml`, set:

```toml
[remote]
address = "100.101.102.103:7433"
key = "desk.key"
folder = "/home/you/code/project" # optional: a folder on the server
```

The key path is relative to the config file. Plain `razor`, `--continue`, `--resume` and `--print` connect to that server without starting one on the laptop; commands such as `razor devices` still manage the laptop's own server. New sessions and `--continue` use `folder` when set, otherwise the path of the directory where you launched Razor; that path must exist on the server. The new-agent chooser can also show the server's past folders. Configure `play` and `capture` on the laptop so its speakers and microphone are used; the server supplies speech and transcription. This connection uses the same private network and full-access key precautions as pairing above.

## More than one Razor on one machine

Each Razor home runs its own server, so one machine can hold two separate Razors, each with its own settings, keys, ChatGPT sign-in and sessions (Claude's sign-in belongs to your user account, so they share it), by pointing `RAZOR_HOME` at a second folder:

```sh
RAZOR_HOME=~/.razor-second razor
```

Give each home its own `[server] address` port if both should be reachable. `razor install` registers each home's background service under its own name, the home's folder without its leading dot (`razor` for `~/.razor`, `razor-second` for `~/.razor-second`), so each home can run as a service; it never replaces a service of the same name that runs another home.
