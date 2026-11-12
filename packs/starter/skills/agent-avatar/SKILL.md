---
name: agent-avatar
description: Make or re-roll a custom avatar for a Razor conversation or resident, a pixel character or an emblem or logo, from a description, inspiration pictures or a logo.
---

# Making an avatar

An avatar is a 132 × 33 PNG of four 33-pixel cells: idle, blink, talk and sleep. Razor draws it in place of that agent's usual creature, using its blink, talk and sleep cells where the creature would use its own. You make one master picture with image generation; `avatar.js`, in this skill's folder, cuts it down to 33 pixels, derives the other three poses, checks the exact format, writes a preview and installs it. A re-roll is the same steps again.

Run the script with `node`. If `node` is not on the `PATH`, use the copy Razor ships: `"$(dirname "$(command -v razor)")/node/bin/node"`.

## 1. Settle what to make

Ask in one message only for what the request leaves open, proposing an answer for each:

- **Subject:** a description, inspiration pictures or a logo. Attached pictures arrive as file paths.
- **Character or emblem:** a character has a face with two eyes, and it closes them to blink and opens a mouth to talk. An emblem (an abstract mark, a logo without a face) glints to blink, swells while it talks and dims to sleep.
- **Who wears it:** a resident, by name; this conversation; or nobody yet.
- **Drawn or reserved:** whether new conversations may draw it at random for their character, or it is kept for whoever it is set on. Tell the person the odds: in the draw it is one more face beside Razor's sixty built-in ones, so with N custom avatars in the pool a new conversation gets each one about one time in 60 + N. Setting it on this conversation or a resident is how to see it now.
- **Name:** letters, digits, `-` and `_`, such as `owl` or `acme-mark`. Propose one from the subject.

Done when all five are known.

## 2. Make the master picture

Pictures are made on the person's ChatGPT sign-in, one image call per roll, about 30 seconds each, counted against their plan's image allowance. Say that once, before the first roll.

- **You have the `imagegen` tool** (`image_gen.imagegen` in code mode): call it with the prompt, `transparent_background: true`, and the person's pictures in `referenced_image_paths`.
- **Your instructions say to start a GPT helper for images:** start a helper with model `sol` and the brief: "Make one image with your image tool, with a transparent background: <prompt>. Use these files as references: <paths>. Make one call, change no files, and report the saved image's path." Settle it with `discard` once its report arrives; the path is all you need.
- **Neither:** this server has no ChatGPT sign-in, so it cannot make pictures. Tell the person that `razor login` signs Razor in to ChatGPT on a paid plan and adds image generation. Meanwhile a PNG of a logo with a transparent or plain one-colour background can go straight to step 3, with no generation.

Prompts that worked:

- **Character:** "Pixel-art sprite of <subject>, front-facing, centred, big chunky pixels, a small flat palette of about 8 colours, no anti-aliasing, a large face with two clear dark eyes on a lighter face, transparent background, nothing else in the frame."
- **Emblem:** "Flat vector icon: <subject>, bold and symmetric, at most three flat colours, broad shapes, a conspicuous centre, no text, transparent background, nothing else in the frame."
- **From a logo:** pass the logo as a reference and name exactly the part to keep; ask to drop its text, background and surrounding ornament.

What spoils a roll: details finer than a thirtieth of the picture (text, thin lines, a subtle expression) vanish; several similar shades merge into one; eyes that touch a dark outline or are no darker than the face are not found; a background with scenery, a gradient or a shadow is refused.

## 3. Reduce it and show the person

```sh
node <skill folder>/avatar.js make <master.png> character <folder>/<name>.png
```

Use `emblem` in place of `character` for an emblem. It writes the sheet and `<name>-preview.png` beside it: the four cells enlarged eight times on a dark ground, left to right idle, blink, talk, sleep. A refusal says what to change:

| Refusal | Next |
| --- | --- |
| no pair of eyes found | Roll again asking for larger, darker eyes clear of any outline, or make it an emblem. |
| no transparent background and its corners differ | Roll again with a transparent background. |
| not a PNG, or not 8-bit | Convert it (`sips -s format png <file> --out <file>.png` on a Mac), or pass it as a reference to an image call. |

Deliver the preview with `deliver`, since the person sees only what you deliver, and ask whether to keep it or roll again: the same prompt for a new take, a changed prompt, or the last master as a reference for the same idea redrawn. Done when the person picks a sheet.

## 4. Install it

```sh
node <skill folder>/avatar.js install <sheet.png> <name> [--pool] [--resident <resident>] [--session "$RAZOR_SESSION"]
```

`--pool` puts it in the random draw; without it the avatar is reserved. `--resident` sets it on a resident, `--session "$RAZOR_SESSION"` on this conversation. The script copies the sheet to a new file in Razor's `avatars` folder and sets it in the person's settings under `brand.avatars.<name>`. Installing again under the same name replaces the picture for everyone wearing it. Earlier files stay, so going back is `razor config set brand.avatars.<name>.file <earlier file>`.

Done when the script prints the installed file's path. The server re-reads pictures when settings change or it restarts. If a file is missing or fails its check then, the server drops that avatar with a config notice, and whoever wore it shows the usual creature again.

## What the person sees

- The terminal app draws it now, in place of the agent's creature in its workshop scene (the small strip shown when the pane is short keeps its own art). It reads best where the scene uses octants, characters that split a cell into eight blocks. In tmux or on a narrow screen the scene uses half blocks, and the avatar is about 11 pixels across and hard to make out; `razor config set ui.blocks octant` keeps octants in tmux.
- The phone and Mac apps draw custom avatars from a later build; until then they show the usual creature.
- A helper wears its top-level agent's avatar, and a resident's threads and runs wear the resident's.
- A conversation draws its character once, when it starts, so a change to the draw leaves existing conversations' faces alone.
- At 33 pixels a face keeps its eyes and shape but loses fine expression. An emblem with no face pulses while it talks rather than visibly speaking.
- Razor keeps at most 16 avatars.
