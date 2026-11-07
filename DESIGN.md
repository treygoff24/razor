---
name: Razor
description: The cockpit for a crew of agents, drawn on a terminal's character grid.
colors:
  # The theme. The terminal draws from these and from blends of them, nothing else.
  glass: "#05080b"
  bezel: "#1d2227"
  bezelHi: "#2b3238"
  recess: "#0b1015"
  strip: "#16202a"
  data: "#dfe6ea"
  white: "#eef2f4"
  gray: "#7d8993"
  grayDim: "#434d56"
  cyan: "#3fd9f2"
  green: "#3ee56d"
  magenta: "#ff5bdc"
  amber: "#ffb319"
  red: "#ff3d32"
  violet: "#cd8bff"
  violetDim: "#704d9a"
  violetHi: "#e6b9ff"
  lamp: "#ecd48c"
  agentBg: "#1a1130"
  youBg: "#123440"
  cyanBg: "#05202a"
  redBg: "#2a0b09"
  amberBg: "#2a1e05"
  addedBg: "#07240f"
  chip: "#1d4a58"
  button: "#0c3a48"
  # The eight colours a person can give a saved group.
  group-slate: "#828f9c"
  group-steel: "#7194a1"
  group-indigo: "#8487b3"
  group-plum: "#a384a6"
  group-rose: "#b4858f"
  group-clay: "#b58f75"
  group-sand: "#b0a478"
  group-moss: "#88a07c"
  # Each group colour as the instrument panel draws it.
  panel-slate: "#88a4c1"
  panel-steel: "#65b2ce"
  panel-indigo: "#868dee"
  panel-plum: "#c581cc"
  panel-rose: "#ec859b"
  panel-clay: "#d9a37f"
  panel-sand: "#d0bf82"
  panel-moss: "#9bbd8b"
  # The instrument panel (approved 6 November 2026).
  panel: "#070916"
  panelInk: "#2b3366"
  panelGray: "#9fa8b0"
  chipInk: "#05060f"
  clock: "#5ef0ff"
  clockBg: "#112a44"
  askWords: "#c9b48a"
typography:
  body:
    fontFamily: "the person's own terminal font, monospace"
    fontWeight: 400
  emphasis:
    fontFamily: "the person's own terminal font, monospace"
    fontWeight: 700
  goal:
    fontFamily: "the person's own terminal font, monospace, italic"
    fontWeight: 400
spacing:
  cell: "1 column × 1 row"
  gutter: "1 column"
  tile-gap: "1 column, 1 row"
components:
  chip-asks:
    backgroundColor: "{colors.amber}"
    textColor: "{colors.chipInk}"
  chip-stuck:
    backgroundColor: "{colors.red}"
    textColor: "{colors.chipInk}"
  tab-razor:
    backgroundColor: "{colors.cyan}"
    textColor: "{colors.chipInk}"
  selected-row:
    backgroundColor: "{colors.cyan}"
    textColor: "{colors.chipInk}"
  tile-asks:
    backgroundColor: "{colors.amberBg}"
    textColor: "{colors.data}"
  tile-stuck:
    backgroundColor: "{colors.redBg}"
    textColor: "{colors.data}"
---

# Design: the Razor cockpit

The visual authority for Razor's terminal app. Written on 6 November 2026. It rests on the Loom cockpit Trey designed, which Razor's terminal was built to reproduce ("the existing look", SPEC section 5), and on the instrument-panel overview and the list beside a session that Trey approved on 6 November 2026 from an interactive prototype ("Yep, this is perfect"). SPEC.md governs behaviour; PRODUCT.md holds product truth; this file governs how the terminal looks and moves. The per-screen design pages under docs/design hold each screen's layout and keys.

## Overview

A cockpit for a crew: dark glass, thin bezels, and a small set of colours that each mean one thing. The person reads state before words: amber and red pull the eye to what needs them, green says work is happening, gray says nothing is. Everything else stays quiet. The overview and the list beside a session add an instrument-panel layer: each top-level group is a framed panel in its own colour, with a number tab, tick marks, a hatched foot and a small activity gauge per agent, on a deep navy ground. It should feel like calm command of many machines, never like a dashboard competing for attention, and it should be a joy to look at: distinctive and playful in its details (the pixel creatures, the big clock, the tick marks and hatching), never generic. Calm is about state; delight is about craft.

The colours in the frontmatter restate the terminal's own colour table (`theme!` in `src/terminal/text.rs`), which is the one source; `[brand] colours` takes the same names.

## Colors

**The colour law** (from the Loom cockpit): "Colour is state, never decoration, and never the only carrier of meaning: white is data, green is engaged, cyan is the human, magenta is where the agent is headed, amber is caution, red is warning." Violet is the agent's own voice. Lamp gold belongs to skills alone.

One difference between Razor's apps is deliberate: on the terminal a question waiting for the person draws amber (caution, the Loom look Trey kept and approved again in the 6 November prototype), while on the phone needs-you is cyan (the human's call, approved for the phone app on 31 October 2026). Both are Trey's; neither app changes to match the other unless he says so. Every region draws with the theme's colours and blends of them made with one mixing function (a colour moved toward another by a fraction, channel by channel); a region that needs a colour not in the theme makes it as a blend and says so.

| Colour | What it means |
| --- | --- |
| glass | the ground of every region |
| recess, strip, bezel, bezelHi | panels, bars and edges, quietest to brightest |
| data | ordinary words |
| white | names and emphasis |
| gray, grayDim | secondary words; grayDim only for rules, ticks and marks, never for words a person must read (2.3:1 on glass) |
| green | engaged: working, success |
| cyan | the human: your words, your call, your keys, the selection |
| magenta | where the agent is headed: its headline, its targets, the clock's seconds |
| amber | caution: a question waiting for you |
| red | warning: stuck, failed |
| violet (dim, hi) | the agent's own voice; hi for a live thought's newest letters |
| lamp | skills |
| agentBg, youBg, cyanBg, redBg, amberBg, addedBg | the grounds of the agent's reply, your message, a selection, a failure, a question, added lines in a diff |
| chip, button | a key's chip and a pressable button |

**Group colours.** A saved group takes one of eight muted colours (slate, steel, indigo, plum, rose, clay, sand, moss). A group's colour says which group it is and never stands in for a state; the agents inside keep their state colours.

**The instrument panel** adds a deep navy ground (`panel`) under the overview, a frame ink for its header (`panelInk`), a lighter gray for secondary words on its tinted grounds (`panelGray`, at least 4.5:1 on every group's tile and row ground), dark ink for words on solid chips (`chipInk`), the clock's bright cyan on its own ground (`clock`, `clockBg`), and a warm gray for a question's words on its amber ground (`askWords`). A top-level group's frame, tab and activity bars are drawn in the group colour's panel form (`panel-slate` … `panel-moss`), a brighter fixed value beside each saved one; its ground is that colour at 13 percent over the panel, its frame at half strength. The panel forms keep each group clear of amber, red and working green, so needs-you stays the only warm alarm on the screen: clay is a peach-tan and sand a khaki, never amber.

**A brand** may replace any theme colour by name (`[brand] colours`). The meanings stay.

**Contrast.** On glass, every ink a person reads holds at least 5.5:1 (gray 5.6, red 5.7, magenta 7.4, data 15.9); on the amber and red grounds gray falls to 4.6:1, still above 4.5. Dark ink on a cyan or amber chip is 11.9:1.

## Typography

There are no fonts to choose: the terminal draws in the person's own monospace font, one character per cell. Hierarchy comes from four things only: colour, bold, italic, and position.

- Bold for names, chips and counts.
- Italic, dimmed, for an agent's goal under its name, so the goal reads as context rather than news.
- Plain data-coloured text for what agents and people say.
- Big digits (the clock) are drawn with block characters: the Fine clock is a 5×7 pixel font in octants (characters that split a cell into eight blocks), two rows tall, with the seconds in magenta and am/pm in gray beside it. Where octants are unsupported (tmux, narrow screens) it falls back to half blocks.
- Labels on frames and chips are uppercase with a space either side (` RAZOR `, ` ◆ 2 NEED YOU `). Everywhere else, sentence case.

## Layout

The screen is a grid of cells. Widths are measured in columns by each character's display width, and text that does not fit is clipped with an ellipsis rather than wrapped, except an agent's status words, which wrap to the room left.

- **Widths:** under 70 columns is a phone-sized terminal, 70 to 109 narrow, 110 and up wide; the side pane docks from 140 columns. Everything holds down to 40 columns.
- **Three views on one line:** the session alone, the agents list beside it, the full-screen overview. Left steps out, Right steps in.
- **The overview** opens with a header of three framed panels (the clock, the questions waiting on you, the counts by state over a coloured bar), then one framed panel per top-level group. Groups sit side by side only where that leaves little empty room; otherwise they stack. Leftover space beside a shorter group is hatched, not left blank. Inside a group, agents are tiles: needs-you first, then working, then idle.
- **The list beside a session** is the same language at 48 to 64 columns: a compact header (the Fine clock, the date, the needs-you tag and the counts bar), each top-level group as a framed panel, each agent two rows (name with its goal in dim italics; what it is doing with a small activity bar), helpers branching underneath. A folded group is one line: name, waiting count, counts by state.
- **Spacing** is one column of gutter inside a frame and one row between tiles; a frame never touches the clock or a word it encloses.

## Elevation & Depth

Depth comes from ground colour, not shadows. Glass is the floor; recess and strip sit one step up; a tile takes its state's ground (amber for a question, red for stuck, a faint green tint for working, a faint gray for idle) over its group's tinted ground. A floating side pane casts a three-column shade on what it covers. The selected agent's name row is a solid cyan bar with dark ink, the one fully solid row on the screen.

## Shapes

- Frames are box-drawing lines with rounded corners (`╭╮╰╯`).
- A top-level group's frame carries tick marks every five columns along its top edge (`┴`), a number tab and a name tab at the top left ending in a slant (`◤`), and along its foot a short hatch (`╱╱╱╱╱╱`) and its index (`··· 01`). The number is the key that jumps to it (1 to 9).
- A tile's left edge is a half-cell bar (`▌`) in its state's colour.
- Activity is twelve bars of eighth blocks (`▁` to `█`), one per five minutes of the last hour, the tallest lit toward white.

## Components

- **State marks**, always beside a word: `◆` amber wants you (asks), `▲` red stuck or error, `●` green working, `○` gray idle, `✓` gray done, and for a helper at work a turning `◐◓◑◒` in the same green (a still `◐` without motion).
- **Status labels** lead an agent's lines: `asks` amber, `stuck` and `error` red, `now` violet (the agent's latest words), `step` gray (the call running and how long), `↳` for what it is doing in a tile.
- **Chips**: a solid ASKS, STUCK or ERROR tag in the state's colour with dark ink, at the right of an agent's name row.
- **The needs-you tag**: ` ◆ N NEED YOU ` in bold amber on a group's top edge or the header.
- **The questions panel**: the oldest open question as an amber bar you can read without opening anything, the next under it, `+N MORE` on the frame when there are more.
- **The counts bar**: counts by state over one bar split in the states' colours.
- **Selection**: a cyan `❯` in the margin and a cyan tint, or in the instrument panel the solid cyan name bar, with a small cyan arrow toward the open session in the list.

## Motion

Every motion is a pure function of its start time and now, and has a still form: with `[ui] motion` off everything draws settled. Lights pace at 83 ms at most; a moving frame (the fold from the overview into the list, about a third of a second, eased out) paces at 33 ms. A screen with nothing moving draws nothing. The helper pulse turns every 180 ms; the clock's minute and an age's next minute are the only redraws at rest. With motion on and the terminal focused, the instrument panel's clock also shows its seconds and steps each second, and a working tile's edge and a question's ground breathe, sampled at the helper pulse's 180 ms and only while such a tile is drawn; with motion off or the terminal unfocused they draw still and the seconds are not shown.

## Do's and Don'ts

- Do give every state its mark and word as well as its colour.
- Do let needs-you be the loudest thing on any screen; nothing decorative may be amber or red.
- Do make a new colour as a blend of theme colours, and say so in the code that draws it.
- Do check every screen at 40, 64, 120 and 160 columns, with motion on and off, in octants and half blocks.
- Don't use a group's colour to mean a state, or a state's colour to mark a group.
- Don't draw a fact the server does not send; ask for the server fact instead.
- Don't put words a person must read in grayDim.
- Don't animate what has not changed.
