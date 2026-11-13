# Release and install

```text
/// Build this checkout's archive and install it; --link also selects it.
scripts/install.sh [--link]
/// Build the current platform's archive, checksum, and versioned installer in DIR.
scripts/release.sh VERSION DIR BASE
/// RAZOR_ACTION="MAJOR.MINOR.PATCH One sentence saying what to do." marks a release needing action.
/// Install the release named in the script; environment overrides select a different one.
/// On creating ~/.local/bin/razor in a terminal, run setup; otherwise print its next command.
/// Preserve stdout's installed binary path for callers that capture it.
curl -fsSL BASE/install.sh | RAZOR_BASE_URL=BASE RAZOR_VERSION=VERSION sh
```

1. Release builds use Debian 12 for Linux x86_64 and aarch64, and the Mac for arm64.
2. Each archive holds the binary, pinned native agent-browser, pinned ripgrep and jq in `tools/` (which `serve` puts last on its agents' `PATH`, so the person's own copies win), and at their source paths the files `scripts/release-files.txt` lists: bridge sources and lockfile, scripts/speech.py, scripts/notify.sh, packs/starter, README.md, SPEC.md, RULES.md and docs/, so agents can read the installed version's docs, and follow their links, offline; missing starter files or docs fail the build. A published archive is built in a checkout of the public release commit, so it carries exactly that commit's docs.
3. The release script records checksums for the archives and generates the installer with the chosen base and version.
4. The installer detects the platform, verifies the archive, and extracts it into a new version folder under the user's data directory.
5. It uses Node 22.18+ from PATH when available; otherwise it verifies and installs a pinned official Node archive inside that version folder.
6. It installs the bridge's npm dependencies at install time and selects the version only after those steps succeed.
7. It links `razor` from `~/.local/bin`, retains the selected and previous versions, folders modified within 24 hours, and any bundle a process owned by the installing user executes from; reruns skip the download and npm for an installed version. Executable paths come from that user's `/proc` entries on Linux and `/usr/sbin/lsof -a -u <uid> -d txt -Fn` on macOS, so a server or bundled Node started through `current` keeps its real bundle. A process-listing failure skips pruning with a sentence on stderr; unreadable executable links are ignored. Other users executing private home bundles, and servers run as system units with `User=`, are outside this promise (4 November 2026).
8. `scripts/install.sh` builds the local archive and invokes the same installer from a file URL; an option preserves the evaluation's unselected candidate.
9. Razor resolves its binary's folder once before starting the runtime, using it for the bridge and private Node; only `serve` adds it to PATH, and `install` writes the stable current/razor path for a release.
10. Only the install that creates `~/.local/bin/razor` enters setup. Print any needed PATH advice on stderr and run setup when stdout/stderr are terminals and `/dev/tty` opens; redirect only stdin there, preserving terminal output. Under `curl … | sh`, stdin is the script and answers come from the controlling terminal. Without it, print the exact shell-quoted `setup` command. Updates print `Run razor restart to use the new version.` An unselected install does neither. Stdout remains one binary path; setup's nonzero exit propagates, leaving the completed install available to rerun. Source `scripts/install.sh --link` follows the same first-link rule.

**Decision: print a PATH command without changing shell files.** If `~/.local/bin` is absent from PATH, print `export PATH="$HOME/.local/bin:$PATH"` for sh/bash/zsh, or `fish_add_path "$HOME/.local/bin"` for fish. For an unrecognized shell, label the sh command and retain the exact executable path. Appending to a profile requires choosing among login and interactive files and cannot update the calling shell. Setup needs no PATH change: serve already prepends its binary's folder. Requiring terminal output keeps captured installer calls noninteractive.

Check the HTTP installer in a clean Linux container and a scratch Mac home, including doctor and agent-browser in an agent shell. Exercise a piped script with a controlling terminal, no controlling terminal, captured stdout, interruption, updates, source installs and unselected installs.

Node on PATH avoids downloading a runtime for most developers; automatically installing pinned Node otherwise removes a first-run barrier at the cost of a roughly 40 MB download and maintaining the pinned checksum. Private Node is used only for the bridge, so it cannot shadow the user's Node in agent shells. Browser installation itself belongs to `razor setup`.

A source build without `--link` requests an unselected candidate from the common installer; installation must not remove existing versions in that mode.

## Updating an installed release

A release never makes a settings file that loaded on the previous release refuse
startup. A retired key loads ignored, with a notice, for at least one release.
A change that code cannot carry over is marked as needing action and called out in
the changelog's "To update" steps. Set `RAZOR_ACTION="0.7.0 One sentence saying what
to do."` when running `scripts/release.sh`; it writes that literal line in `install.sh`.
A mark is never dropped, only superseded; publishing refuses an unmarked installer
when the latest release has a mark. Its version is when the newest need began.
For a second need, raise that version and retain each need in the sentence with its
version: "Updating from before 0.7.0: … From before 0.9.0: …". This holds servers
between the two needs too. The sentence is at most 200 characters on one line,
without double quotes, backslashes, dollar signs or backticks.
Razor never rewrites a person's settings during an update: it tells them what
changed and what to do.

```rust
/// Install a newer stable release; a background caller supplies its notification settings.
/// Background updates hold releases needing action; the person's update selects them.
/// Source installations and concurrent updates are refused without changing the installation.
pub async fn update_release(config: Option<&Config>) -> Result<String, Error>;
/// The newer selected release awaiting this process's restart; absent on source installs.
pub(crate) async fn release_ready() -> Option<String>;
/// A pending action and update commands, read from the install root.
pub(crate) async fn release_action() -> Option<String>;
/// The selected release binary for starting a server; source installs keep their own binary.
pub async fn selected_executable() -> std::io::Result<PathBuf>;
```

A release lives directly under the canonical data directory as `MAJOR.MINOR.PATCH`,
with `current/razor` present; source builds use git SHA folders or live outside this root.
A release check reads the generated installer's stable `MAJOR.MINOR.PATCH` declaration.
Only a version newer than both the running and selected versions is installed.
The installed binary embeds the installer and runs it under one per-install lock.
The downloaded installer supplies only the version, download URL and optional action mark.
Versions compare numerically as three unsigned numbers; leading zeroes, prereleases
and build suffixes are not stable release versions. The action line must match
`RAZOR_ACTION="…"` and begin with a stable version and a nonempty sentence. A malformed
mark holds the release and shows its text as it is; it never permits automatic selection.
When the newer of the running and selected versions predates the mark, a background
check neither downloads nor selects the release. It saves the mark in one small
`release-action` file under the install root. `current` stays unchanged: a restart,
reboot or crash still starts the selected old version. A typed `razor update` downloads
and selects the latest release and prints the action. After that successful install
it removes the file. A check that finds the latest release already installed also
removes the file, even if its mark is malformed; status hides a met mark meanwhile.
Installer improvements reach an existing installation one release later; downloaded shell
code is never executed. Every release archive must install correctly with the previous
release's installer.
Its checksum, dependency installation, atomic selection and retention stay authoritative.
A failed check or install leaves the running server and selected version available.
The residents loop starts checks on its first wake and daily, using its existing clock.
`release_updates = false` prevents future automatic checks; a running check finishes.
The server status line keeps `release_ready` for a selected version awaiting restart.
An optional `release_action` carries the pending action's full sentence; older clients
ignore it. The terminal announces a new sentence and retains it with the machine's
name in the status pane; doctor reads the same sentence from the file, even offline.
The notify command gets it when the file is first written or its sentence changes,
without a session's environment. Daily checks and server restarts reuse the file
without another notification. No release bundles accumulate while action is pending.

Servers on 0.6.0 and earlier ignore the action mark and still select releases
automatically, as does any server sharing their install folder. The hold protects
updates only from releases containing this code; it cannot protect a 0.6.0 server's
first update to one of those releases.

The hard decision is selecting `current` while a server runs. The server already freezes its
canonical install folder before starting its runtime, and the installer retains folders with
live executable paths. Reusing its selection preserves those guarantees and lets the existing
service start the new release at the person's next `razor restart`. Detached servers also
start the selected binary. No automatic restart interrupts a session.
