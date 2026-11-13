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

1. **Claude, ChatGPT, both, or an API key.** Enter means Claude. Choosing ChatGPT alone makes `sol` your default model, unless your settings already name one. Choosing a key asks whose: Anthropic, OpenAI, Google, RunInfra, or another endpoint, such as your company's AI gateway, whose API, address, model id and context window you give. You type the key hidden; setup saves it in `~/.razor/keys.toml`, readable only by you, and makes that provider's model your default ([Models and sign-in](models.md#providers)).
2. **Contact nothing but your model provider.** Asked after an API key, for work machines: yes turns on `provider_only`, so Razor itself reaches only the providers in your settings ([Security](security.md#contacting-only-your-model-provider)). The default is no.
3. **Run Razor in the background when you log in.** Say yes to have the server start at login, which always-on agents need. You can run `razor install` later.
4. **Sign in with your provider.** Setup skips providers already signed in, and an API key needs no sign-in. Claude uses your subscription; to use Anthropic Console billing, run `razor setup --console`, which signs Claude in again even when an account is already signed in. For Claude, a browser opens, or you visit the link it prints and paste the code back. For ChatGPT it prints a link and a code to enter at OpenAI.

To repeat setup, run `razor setup`. It keeps existing settings and sign-ins.

Razor ships the browser and desktop-control programs; setup downloads Chrome (unless `provider_only` is on) and runs `razor doctor`, which prints what it found and a command for each problem, then whether your default model can run; on an API key it sends that model one short test request first:

```text
Razor can run on opus.
```

If your default model cannot run, setup stops there; fix what doctor names and run `razor setup` again.

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

To start with the agents list open beside the conversation you last wrote in (or a fresh conversation if there is none), run `razor --agents`. Left at the empty composer opens the same list after startup, as F2 and `/agents` do, and Left there again opens the overview: every agent on the whole screen, with what it is for, what it is doing now and its helpers. There Up and Down choose an agent, and Right, Enter or a click opens it beside the list; Right at the empty composer then closes the list and leaves the session on the whole screen, as F2, `/agents` or a click on the ‹ at the list's top right does. In a helper's conversation, Left goes back to its parent. F3, or the ⤢ at the list's top right, also opens the overview, and F3 or Esc goes back. To move between agents and type to each one with the list staying open, you can bind a second set of keys instead: Shift-Tab or Left to give the list the keys, Up and Down passing over helpers, and Enter or Right opening an agent beside the list ([Moving between agents](configuration.md#moving-between-agents)). Many actions also sit behind a prefix, as in tmux: with an empty draft, press backtick or Ctrl-B, then a key; the prefix and `?` list every key ([Terminal keys](configuration.md#terminal-keys)). To jump to any agent instead, `/goto` (or the prefix, then `g`) opens Go to: every agent on every connected machine by project, children under their parents. Press `/` and type part of a name, a topic or a state such as `working`; the same words also search your past conversations, listed underneath. Arrows or j/k choose, Enter opens it, Esc closes.

To find words in the conversation you are addressing, use `/search <words>`. Words can appear in any order, and each may be the start of a word. Arrows or j/k select a result; Enter opens its place in the transcript. Press `/` in the list to refine the words and Enter to search again. `/search` alone returns to the last list. Esc closes the list, and Ctrl-End returns the transcript to its newest entries. The list holds at most 50 rows; add a word to narrow a full list.

To ask one question without opening the terminal app, use `razor --print`: it sends the message, prints the answer and exits, which also suits scripts:

```sh
razor --print "List the TODO comments in this repository."
```

[Using Razor from scripts](scripting.md) covers its JSON and streamed output, carrying a conversation across calls, and the API behind it.

To connect another machine, run `razor devices add <its name>` on one, which shows a code, and `razor devices join <the first one's name>` on the other, which asks for it. [More machines](machines.md) covers pairing, relays, the phone and Mac apps and what access a paired server gets.

Next: [Concepts](concepts.md) explains sessions, helpers and memory; [Configuration](configuration.md) lists every setting.

## Update Razor

Run `razor update` to install the latest release, then `razor restart` when you want the server to use it. A release install also checks at server startup and about once a day. It downloads a newer release while your sessions keep running, and the terminal shows a one-line notice when it is ready. Restarting is your choice.

If the release is marked as needing you to act, the background check holds off downloading
it and keeps your current version selected, so a restart or reboot still uses that
version. The terminal and your notify command say what to do once; the action stays
in the terminal's status pane and `razor doctor`. Run `razor update` to download and select the latest
release and print the action again, then `razor restart`. Servers on 0.6.0 and earlier,
or any server sharing their install folder, still select the next release automatically.

Run `razor config set release_updates false` to stop future background checks; `razor update` still works. Source builds update from their checkout. On v0.1.0, run the [install command](#1-install) once more and restart to get these update commands.
