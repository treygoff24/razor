---
name: razor
description: Set up, change, explain or debug Razor itself: settings, models and API keys, tools, MCP servers and skills, devices, notifications, updates and the server.
---

# Razor

Read the reference for the job, then the installed documentation it names. Check the actual home, settings, version and error before changing anything; keep unrelated settings and secrets intact. Verify the requested behavior, or report what remains unverified. For always-on agents, use the `residents` skill.

- [Settings](./references/settings.md): config files, precedence, startup, about, terminal keys and daily voice and call limits.
- [Models and keys](./references/models.md): sign-in, API keys, providers and `provider_only`.
- [Tools and skills](./references/tools.md): MCP, script tools, plugins and skills.
- [Other agents](./references/other-agents.md): bringing MCP servers, skills, plugins and tools over from Claude Code, Codex, OpenCode, pi, Code Puppy or any other agent.
- [Troubleshooting and updates](./references/troubleshooting.md): doctor, logs, the running binary, restart and release updates.
- [Devices and notifications](./references/devices.md): phone notifications, network access and pairing.

The installed docs index is normally `~/.local/share/razor/current/docs/README.md` (replace `~/.local/share` with `$XDG_DATA_HOME` if set); `razor --help` also gives its location. For a source build use the checkout's `docs/`. For what changed in an update, also read `CHANGELOG.md` beside `docs/`. The [online docs](https://github.com/treygoff24/razor/tree/main/docs) may describe a newer version. In these references, doc filenames mean pages in that installed `docs/` folder. `razor --help` and `razor help <command>` show the installed commands.
