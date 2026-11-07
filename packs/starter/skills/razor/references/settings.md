# Settings

Read `configuration.md` for the exact settings, defaults, scopes and restart exceptions. `razor config get <key>` shows the effective value and source; `razor config set <key> <value>` and `razor config unset <key>` edit the person's file. Inspect the existing file before a direct edit and verify the effective result afterward.

Razor layers the selected pack's `config.toml`, `$RAZOR_HOME/config.toml` (default `~/.razor/config.toml`), the project's `.razor/config.toml`, then `.razor/config.local.toml`. Later settings win; top-level tables merge one level, named entries and lists replace whole. Machine and person settings are refused in project files. Relative paths count from the file naming them. Do not edit the shipped pack: upgrades replace it. If changing `startup`, copy the whole list and its relative files beside the new setting.

Most changes apply on the next turn; terminal key changes apply while the terminal is open. Ask the person to run `/keys` in the terminal to see effective bindings and action names; `configuration.md` under Terminal keys lists contexts and actions. For `about.md`, use the person's own words and change it only at their request. See `configuration.md` for key syntax, errors, and settings needing restart.
