---
name: cua-driver
description: Desktop apps from the shell. Find an app's window, read it as an accessibility tree with element tokens, click, type, pick menu items and check the result, take window screenshots; for native apps, not web pages.
---

# cua-driver

`cua-driver` is Cua Driver's command line, installed with Razor. It reads and operates desktop apps through the operating system's accessibility interface. Razor runs its daemon; every call reaches it through your home's socket:

```bash
cua-driver --socket "$CUA_DRIVER_SOCKET" <tool> '<json arguments>'
```

Each call prints JSON. Put the same `session` label in every call of a task, so tokens from one call work in the next. For a web page, use agent-browser instead.

## Before you start

- **It can take over the screen.** Cua Driver acts in the background where it can, but windows can still come forward, flash or take the keyboard, and on a Mac it has taken the person's screen and mouse. Tell the person before a desktop task.
- **A `screen_locked` refusal** means the Mac is locked: reads come back empty and input is refused until the person unlocks it, so tell them rather than retrying.
- **"Daemon is not running"** means desktop control is off here. On a Mac, ask the person to run `razor doctor`, which requests Razor's permissions; on Linux the server needs a display. Never start, stop, update or reinstall Cua Driver yourself (`serve`, `stop`, `update`, `check-update`, `skills`, `install_*`).

## Find the window and read it

```bash
cua-driver --socket "$CUA_DRIVER_SOCKET" list_windows '{"session":"t"}'    # each window's pid and window_id
cua-driver --socket "$CUA_DRIVER_SOCKET" launch_app '{"session":"t","name":"TextEdit"}'
cua-driver --socket "$CUA_DRIVER_SOCKET" get_window_state '{"session":"t","pid":844,"window_id":10725,"query":"Save","include_screenshot":false}'
```

Row `[N]` of a read is the element token `"<snapshot_id>:N"`, such as `"s0000002a:11"`; a newer read of the window replaces the old tokens. Leave out `query` for the whole tree, and add `"since":"latest"` to get only what changed since your last read.

## Act by element

```bash
cua-driver --socket "$CUA_DRIVER_SOCKET" click '{"session":"t","pid":844,"window_id":10725,"element_token":"s0000002a:11"}'
cua-driver --socket "$CUA_DRIVER_SOCKET" type_text '{"session":"t","pid":844,"window_id":10725,"element_token":"s0000002a:4","text":"Ada Park"}'
cua-driver --socket "$CUA_DRIVER_SOCKET" set_value '{"session":"t","pid":844,"window_id":10725,"element_token":"s0000002a:2","value":"Data"}'
cua-driver --socket "$CUA_DRIVER_SOCKET" press_key '{"session":"t","pid":844,"window_id":10725,"key":"Return"}'
cua-driver --socket "$CUA_DRIVER_SOCKET" invoke_menu '{"session":"t","pid":844,"window_id":10725,"path":["File","Save As…"]}'
```

- Type into text fields with `type_text` and the field's token. On Linux, `set_value` on a text field can also press the window's default button, which submits a form; keep `set_value` for lists, combo boxes and sliders.
- Several known actions go in one call: `run_steps` with `"steps":[{"tool":"type_text","args":{...}},{"tool":"click","args":{...}}]` and `"observe":{}` runs them in order, stops at the first failure and returns what changed.
- Check at checkpoints with a targeted read or `verify_state`. `"effect":"unverifiable"` is not success; a refusal's `hint` and `next_call` say what to do next. Never repeat an action whose outcome you do not know without reading first.

## Keep out of the person's way

- Open apps with `launch_app`, never `open`, `osascript ... activate` or `cliclick`, which bring them forward.
- Prefer element actions and `invoke_menu` to clicks at pixel coordinates.
- Use `"delivery_mode":"foreground"`, `bring_to_front` or the whole desktop as a target only when the person has agreed that you take the screen. A `background_unavailable` refusal means try an element action or ask, not escalate.
- When you finish, `end_session` with your label; leave the person's apps open unless asked.

## Screenshots

```bash
cua-driver --socket "$CUA_DRIVER_SOCKET" --screenshot-out-file /abs/path/shot.png get_window_state '{"session":"t","pid":844,"window_id":10725}'
```

The picture is a file; open it with a tool that shows you images. Screenshots and trees can hold private information.

## More

`cua-driver describe <tool>` shows a tool's arguments and `cua-driver list-tools` lists every tool. `get_guide` returns Cua's own guidance on a topic (`overview`, `drag`, `browser`, `permissions`, `batching`, `menus`): `cua-driver --socket "$CUA_DRIVER_SOCKET" get_guide '{"topic":"menus"}'`. Where it says to install, update or grant anything, this skill's rules above win.

Written from Cua Driver's own guide (MIT, Cua AI, Inc.).
