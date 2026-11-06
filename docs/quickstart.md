# Quickstart

This page takes you from nothing installed to a helper agent's work merged into your repository. You need Linux (x86_64 or arm64) or macOS on Apple silicon, curl, trusted CA certificates, git, and a Claude subscription or a ChatGPT login. On a minimal Debian or Ubuntu system, ask an administrator to run `apt-get update && apt-get install curl ca-certificates git sudo` first.

## 1. Install

```sh
curl -fsSL https://github.com/treygoff24/razor/releases/latest/download/install.sh | sh
```

The first install starts setup in your terminal. On a machine without a terminal, it prints the command to run later: `~/.local/bin/razor setup`. If `~/.local/bin` is missing from your `PATH`, it prints the command to add it. For sh, bash and zsh:

```sh
export PATH="$HOME/.local/bin:$PATH"
```

For fish, use `fish_add_path "$HOME/.local/bin"`. Save the change in your shell's profile if you want it at every login. Building from source is in the [README](../README.md#install).

## 2. Set up

Answer setup's questions:

1. **Claude, ChatGPT or both.** Enter means Claude. Choosing ChatGPT alone makes `sol` your default model, unless your settings already name one.
2. **Run Razor in the background when you log in.** Say yes to have the server start at login, which always-on agents need. You can run `razor install` later.
3. **Sign in with your provider.** Setup skips providers already signed in. Claude uses your subscription; to use Anthropic Console billing, run `razor setup --console`, which signs Claude in again even when an account is already signed in. For Claude, a browser opens, or you visit the link it prints and paste the code back. For ChatGPT it prints a link and a code to enter at OpenAI.

To repeat setup, run `razor setup`. It keeps existing settings and sign-ins.

Razor ships the browser and desktop-control programs; setup downloads Chrome and runs `razor doctor`, which prints what it found and a command for each problem, then the models Razor can run on:

```text
Razor can run on Claude.
```

If doctor finds no model Razor can run on, setup stops there; fix what doctor names and run `razor setup` again.

On minimal Linux systems, Chrome also needs system libraries. Install them with `~/.local/share/razor/current/agent-browser install --with-deps`; this uses sudo. Doctor checks model sign-ins, keys, the browser and desktop-control programs, the server and the service file. It does not check whether Chrome can launch; `~/.local/share/razor/current/agent-browser doctor --offline` does.

Say yes to the last question to open the terminal app, or open it yourself in the next step. Optional web-search keys (Exa, Firecrawl and Parallel) and voice keys (ElevenLabs) come later:

```sh
razor keys
razor restart
```

Key entry is hidden; Enter keeps a saved value. When you create the ElevenLabs key, give it the Text to Speech and Voices (read) permissions (or leave it unrestricted): speech lists voices and turns text into speech. Without these keys, Claude and GPT agents still have their model's own web tools, and nothing is read aloud. To have every conversation start knowing your name or preferences, ask an agent to put them in your about file, `~/.razor/about.md`.

## 3. Send your first message

Open Razor inside a git repository, so the agent and its helpers have one to work in:

```sh
cd ~/code/your-project
razor
```

Type a message and press Enter:

```text
What does this repository do? Answer in five lines.
```

The reply streams into the transcript. Escape stops the agent mid-turn, along with the command it is running. `/help` lists every key and command. Pictures appear in the transcript too: the ones you attach, and the ones Razor's own tools return to an agent, such as a generated image or an MCP server's screenshot.

## 4. Ask for a helper

A helper is an agent your agent starts for one task. It gets a name, a brief (the task written out for it), a fresh conversation, and its own copy of your repository on its own branch, so it never edits the files you are working in. Razor's screens, settings and branch names call a helper a child. Ask for one in plain words:

```text
Start a helper to add a short usage section to the README. When it reports, review its branch and merge it.
```

A card for the helper appears in the transcript, showing its name, its state (working, needs you, done, stuck) and what it is doing. To watch it, click the card or type `/attach <its name>`; type a message there to steer it, and press Escape to go back. Ctrl-N and Ctrl-P move between the cards.

The helper works in a git worktree, a second checkout of your repository, at `~/.razor/sessions/<its id>/worktree`, on a branch named `child/<its id>`. It commits there and reports to its parent.

## 5. Merge its branch

When the report arrives, your agent reads it, looks at the branch, and decides what happens to the work: it merges the branch into the branch you have checked out, or discards it, or records that it was merged by hand. Razor calls this decision settling the helper. A merge that conflicts is aborted and comes back to the agent in git's words, and your checkout is left as it was. After a merge Razor removes the helper's worktree and its merged branch.

Check the result in your repository:

```sh
git log --oneline -3
```

The helper's commit is now in your branch's history.

## 6. Quit the terminal app, come back, or work without it

Press Ctrl-C twice to quit the terminal app; with words in the draft, the first Ctrl-C empties it. The session keeps running on the server, and its helpers with it. To reopen the newest session in a folder:

```sh
razor --continue
```

To start with the agents list open beside your most recently active conversation (or a fresh conversation if there is none), run `razor --agents`. Left at the empty composer opens the same list after startup, as F2 and `/agents` do, and Left there again opens the overview: every agent on the whole screen, with what it is for, what it is doing now and its helpers. There Up and Down choose an agent, and Right, Enter or a click opens it beside the list; Right at the empty composer then closes the list and leaves the session on the whole screen, as F2, `/agents` or a click on the ‹ at the list's top right does. In a helper's conversation, Left goes back to its parent. F3, or the ⤢ at the list's top right, also opens the overview, and F3 or Esc goes back. Many actions also sit behind a prefix, as in tmux: with an empty draft, press backtick or Ctrl-B, then a key; the prefix and `?` list every key ([Terminal keys](configuration.md#terminal-keys)). To jump to any agent instead, `/goto` (or the prefix, then `g`) opens Go to: every agent on every connected machine by project, children under their parents. Press `/` and type part of a name, a topic or a state such as `working`; the same words also search your past conversations, listed underneath. Arrows or j/k choose, Enter opens it, Esc closes.

To ask one question without opening the terminal app, use `razor --print`: it sends the message, prints the answer and exits, which also suits scripts:

```sh
razor --print "List the TODO comments in this repository."
```

[Using Razor from scripts](scripting.md) covers its JSON and streamed output, carrying a conversation across calls, and the API behind it.

To connect another machine, put both on the same Tailscale network and run `razor setup` on each. Setup prints this server's hostname and reachable address without replacing values you set. `razor devices find` finds the other servers and prints the command to pair one; run the confirmation command on the other machine too. [More machines](machines.md) covers pairing, manual addresses and what access a paired server gets.

Next: [Concepts](concepts.md) explains sessions, helpers and memory; [Configuration](configuration.md) lists every setting.

## Update Razor

Run `razor update` to install the latest release, then `razor restart` when you want the server to use it. A release install also checks at server startup and about once a day. It downloads a newer release while your sessions keep running, and the terminal shows a one-line notice when it is ready. Restarting is your choice.

Run `razor config set release_updates false` to stop future background checks; `razor update` still works. Source builds update from their checkout. On v0.1.0, run the [install command](#1-install) once more and restart to get these update commands.
