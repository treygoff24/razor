# The agents view

The list beside a session, drawn in the instrument-panel look of the full-screen overview: a compact header, each top-level group as a framed panel in its own colour, each agent on two rows with a small gauge of its last hour. Trey approved it on 6 November 2026 from an interactive prototype, at 64 and 48 columns ("Yep, this is perfect"); [DESIGN.md](../../DESIGN.md) holds its colours and shapes. It supports three group levels and configurable terminal keys; Trey removed its previews on 3 November (see Opening). The session beside it is drawn as it always was.

Group definitions remain in the server's saved views; the list resolves nothing of its own.

## Layout and rows

The list's whole column is the panel's deep navy (`panel`), with no outer border; its right column is the edge a drag resizes.

- **The header** is four rows in a `panelInk` frame with tick marks every five columns along its top and a short hatch on its foot. On its top edge, the `RAZOR` tab and the `AGENTS · <view>` tab naming the selected view at the left and, at the right, `‹ F2  ⤢ F3` with their effective keys: a click on `‹` closes the list as F2 does, on `⤢` opens the overview. Inside: the Fine clock (two rows of octant digits, half blocks without octants), beside it the weekday and date in white bold, under them the seconds in magenta (with motion on), the census by state as marks and counts (`◆4 ▲2 ●14 ○10`), then the counts bar to the right edge. At the right of the first row, ` ◆ N NEED YOU ` in dark ink on amber while anything needs the person. Parts that do not fit the width drop from the right: the bar, then the census, then the tag's words.
- **The servers line** follows the header as before, and the residents and their threads follow it as a framed panel of their own (see A resident's threads), then `+ New agent` while the list has the keys.
- **A top-level group** is a framed panel in its group colour's panel form, on its ground at 13 percent: on its top edge, the number tab (the key 1 to 9 that jumps to it), the name tab ending in `◤`, ` ◆ N ` in amber while its agents need the person, and its agent count at the right; on its foot, the hatch and its index (`··· 01`). Its sides run past every row inside it. Folded, a group is that one top line: its tabs, its waiting count, its census by state and `N AGENTS ▸`, the frame's corners open.
- **A subgroup** (depth 1 or 2) is a heading row inside its group's frame: its fold mark (`▾` open, `▸` folded), its name in capitals and bold in the group's light ink, a rule in the group's colour, and its census by state at the right. Subgroups after the first stand a blank row apart. A folded subgroup is its heading alone.
- **An agent** is two rows. The name row: the state mark (`◆` asks, `▲` stuck or error, `●` working, `○` idle), the bold name, its goal (the headline, else nothing) in dim italics, and at the right an `ASKS`, `STUCK` or `ERROR` chip in dark ink on the state's colour, or with its helpers folded their marks, then its age. The status row: `↳` and what it is doing (its question in light amber, its error or the stuck sentence in light red, its latest words while working, or “closing after this turn” once it is closing), or `↺` and its latest reply dimmed in italics while idle (with none, its headline, then "No reply yet"), and at its right end the last eight five-minute bars of its hour of calls. The row's ground is its state's: amber for a question, red for stuck or failed, else the group's colour at 11 percent working and 5 percent idle; a `▌` in the state's colour runs down its left edge.
- **Helpers** branch under their root, one row each: `├` or `└` in the group's colour under the root's name, a step further at each depth, the turning `◐` (working), `◆` (waiting on the person or for its merge) or `✓`, the name, and its state in words (`working`, `waiting on you`, `waiting for merge`). A parent's finished helpers past its newest three fold into a `✓ N done` row, opened by its press. A parent's own fold hides its helpers' rows; their marks then sit on its name row.
- **The session being addressed** has its name row as a solid cyan bar with dark ink and `▶` for its mark, and a small cyan `▶` on the frame's right side pointing to the session. The keyboard selection is a cyan `❯` in the margin between the frame and the row, or the fold mark turned cyan on a heading.
- **The last row** of the column names the keys (`↑↓ move  ⏎ open  ␣ fold  1-9 group  ← overview`, from the effective bindings); while the addressed agent is scrolled out of sight it says `▲ talking to <name>` or `▼ talking to <name>` instead, in cyan.

Counts include every listed descendant of the group, including locally folded rows, and exclude settled helpers. Count roots as agents and descendants as helpers. A parent group sums its own rows and every child group, without counting NEEDS YOU or PINNED copies twice. Folded groups keep their counts.

The NEEDS YOU and PINNED shortcuts and Earlier are drawn in the same grammar, as panels without a number. Empty groups, including a parent with no agents anywhere below it, are hidden. Loading, unavailable views and an empty list each get one plain status line. Earlier stays folded by default.

At wide terminal sizes the list starts at a third of the screen, between 32 and 64 columns, beside the session; dragging its right edge, or `<` and `>` (`list.narrower`, `list.wider`) by four columns, sets its width, at least 32 and leaving the session 40, and the home's `list-width` keeps it for the next start. Below 80 columns the list or session fills the screen according to which owns keys. Names clip before state and words; clipping uses terminal-cell widths, and the goal drops before the name.

The server includes a built-in `residents` section before the other sections. Each home is a depth-zero row; its live helpers and open threads the person spoke in follow at increasing depths, including helpers on a directly paired server. Threads use their resident as their display parent without changing recorded parentage or delivery routing. Finished and settled resident activity leaves the section and never moves into folder groups or Earlier, except a person's open threads, which stay as [below](#a-residents-threads); an active descendant of a completed helper uses the next visible ancestor. A hidden fresh run and its descendants stay out of this section. Resident homes remain visible through the existing resident metadata, and the list renders their descendants through the same helper rows as the saved groups. Tree navigation follows the resolved row depths.

## A resident's threads

Approved by Trey on 3 November 2026: "I wanna be able to make separate fresh context threads with residents ... TUI agents view should live and feel excellent". A thread is an ordinary session started as a resident ([residents-api.md](residents-api.md)); it needs no server path of its own.

```
╭─┴ RESIDENTS ◤─┴────┴────┴────┴── 1 RESIDENT ╮
│ ▾ ○ ember                       night 03:00 │
│     + New thread                            │
│     └ ○ tc-five  flow done               3m │
╰╱╱╱╱╱╱───────────────────────────────────────╯
```

- **Rows.** A resident's row has a fold arrow like a parent's: Space, `h`/`l` and a click on the arrow fold its block, `l` unfolds it, and a folded block that hides the addressed thread tints the resident's row. Under it sits the "+ New thread" row, then its threads as the same one-line rows helpers use, branch mark and all, so selection, click, Enter and the addressed tint work unchanged. A thread's clock is the time since it last did anything, as a root's is.
- **Which threads.** The server's Residents section lists a resident's open threads, the sessions with a person's input in them, whether or not they are live, most recently active first. A closed thread leaves its resident for Earlier, as a closed root does (Trey, 4 November 2026: resident threads "don't go away when closed"; until then the three most recently active closed ones stayed, the rest behind a finished line). An idle close is not the person's, so its thread stays. A fresh run is a thread nobody spoke in, so it stays out of the section. A thread just created appears once its first input from the person lands.
- **After a restart.** Only a session with work in flight (or idle maintenance due) is loaded when the server starts, so every idle session used to be absent from the status until something read it. At start the controller now reads, in a background task and without starting any session task, the summaries of the roots whose records changed in the last twelve hours (the idle-root rule's window; a stat of the record decides before any of it is folded) and of each resident's twelve most recently changed threads, whatever their age (`Controller::recent`). Job threads, children and older roots stay out. The agents list after a restart therefore holds what the layout shows, with no client asking. Measured on release builds against hard-linked copies of two real homes' sessions (517 records, 824 MB, 45 roots in the window; 199 records, 274 MB, 23 rows), with no live markers: the API answers in 0.08 s before and after; the rows settle in 0.44 s with 0.27 s more CPU and 4 MB more peak memory on the first, 0.18 s, 0.11 s and 7 MB on the second.
- **Starting one.** Enter or a click on "+ New thread", `n` with a resident or any of its threads or their helpers selected (`list.thread` in `keys.toml`), or `/thread [resident]` aims the right pane's composer at a new thread: its top bar says "New thread · ember", its placeholder "New thread with ember", the list row keeps the `❯`. Like a new agent, the session is made by the first message (`POST /v1/sessions` with `resident`, through `?server=` for a paired resident), so an abandoned aim leaves nothing behind. `/thread` with no name uses the selected row's resident while the list has the keys, else the addressed session's; anything else gets one sentence naming the way. The server takes the folder from the resident, and stores the thread's name as `<resident>-<noun>` (`ember-ferry`): the noun is the last word of the new id's word name, from `names.txt`, and a name already in use (a listed agent's, or a root's changed in the last week) gets the lowest free `-2`, `-3` …, as a root's does. The person's or the resident's rename replaces it; a fresh run's thread is renamed `<schedule> · <date>` at once, since that says more.
- **The list's stops.** A parent's own fold is not an arrow stop: before, each parent card, and now each resident, held an invisible stop between the row above and itself.

A resident's finished line and its start-time threads count the most recently active, not the newest made, and a voice note as a thread's first message is made on the resident's own server by the same route as typed words (`inputs::creation`).

## Shared groups and filing an agent

One saved `Views` definition belongs to the configured shared server. Other servers forward GET/PUT once and resolve those definitions against their local and directly paired agents. The existing version check prevents one editor overwriting another's save. Retain the 256 KiB stored-definition limit, 8 MiB layout limit, complete view-name order, changed-layout streaming and at-most-once-per-second publication.

Validation and layout resolution use bounded depth-first walks supporting depths 0, 1 and 2. A fourth level is refused. Assign missing stable ids and reject duplicates at every level. `Section.depth` already carries the third level, so no new group-path field goes on a session or its record. An empty parent whose descendants are populated must remain in the layout. Remove a group only when its entire subtree is empty.

`/group Venture / Project / Sub-project` files the addressed root into that existing group in the selected view. Paths have one to three names, with optional spaces around `/`; an unknown or ambiguous path is refused with a sentence pointing to `/views`. A helper files its root, because helpers follow roots. The command reads the current definition, removes that root's session claim elsewhere in the selected view and adds the exact session claim to the destination, then PUTs with the original version. Existing folder and repository claims remain available for automatic placement. The command creates no groups and retries no conflicting save; `/views` remains the editor for names, colours, claims and ordering. `/group` with no path shows the addressed root's current group.

Matches retain their specificity: session, folder, repository; ties choose the first group in depth-first order. Stable ids preserve folds through renames. Fold handling uses each section's depth, closing the complete subtree at any of the three levels. Built-in NEEDS YOU, PINNED, Everything else and Earlier retain their current partition and order. Closed roots go to Earlier immediately; unread deliveries stay in the inbox. Settled children leave and their children become roots. Finished helpers past their parent's newest three remain behind that parent's finished line.

**Smart sort, 2 November 2026.** `Group.why: Option<String>` is carried into its layout section; PUT refuses explanations over 200 UTF-8 bytes or containing a newline. `View.wake: Option<SessionId>` opts Auto into automatic sorting and names its resident; absent is off. The existing once-per-second resolver hands the resolved snapshot to Residents without waiting for admission. A single resident delivery task batches new depth-zero Everything else rows into one `smart-sort` program input; while admission waits for queue space, a new snapshot cancels that admission and retries from the latest roots and current resident membership. Only admitted roots count as sorted. A blocked resident never stalls layout publishing. The wake names the resident's local home session; only a configured resident receives admission. Successfully admitted ids are remembered for this process while retained in the bounded agent snapshot, so a restart or a root disappearing and returning can wake again. Other views, children, Earlier roots and residents cause no wake. No model turn polls for new roots. The resident is personal pack data; version conflicts and preserving human edits are its instructions.

## Opening

The right pane shows only the addressed session, `App.focus`. Clicking or pressing Enter on an agent switches to it through the existing guarded focus path, which saves the old session's draft and refuses while a note or input is pending. Nothing else changes the shown session. Only a group's arrow folds the group.

**Closing.** F2 opens the list with the keys and closes an open list in one press from either side, as `/agents` does, leaving the addressed session on the whole screen. Nobody could find that: Trey, 5 November 2026, "also need a way to close the left side agents view to re-focus on just one session if user wants. left arrow opens it, there's then no way to re-collapse it". So the list's top border carries it beside the overview's mark, each with its effective key (`‹ F2  ⤢ F3`), and a click on `‹` closes the list as F2 does.

**The arrows.** From the composer, Left at the start of a line opens the list beside the session with the composer keeping the keys, and Right at the end of the draft closes it again, leaving the session alone; a second Left opens the overview, and on a helper Left goes to its parent instead. Trey, 5 November 2026: "left arrow key in a noraml session opens the existing agents view. left arrow key again opens this new full screen view. [...] right again, agents pane goes away, just the session transcript and status pane now, left, back to agents pane." Until then Left at the start of a line gave the list the keys. F2 and the prefix's `h` still do, for browsing the list with the arrows.

There are no previews. The list used to show a session's conversation, with a Preview label, while the pointer rested on its row for 500 ms or the selection rested for 300 ms, leaving the composer on its recipient. Trey removed that on 3 November 2026: "pls remove the thing where whenever I mouse over an agent it swaps to that agent in teh session feed, i don't like it, no previews, i might revisit one day". The mechanism was deleted rather than switched off: the list's preview target and deadlines, the hover handling, the Preview label and the preview's place among the card readers. The terminal now asks the terminal emulator for presses, drags and the wheel only, not bare pointer motion, so a terminal no longer sends a report for every pixel the pointer moves.

Keyboard navigation in the list uses j/k and Up/Down, which move the highlight and nothing else. Space toggles the selected heading or parent's children. h/Left moves to the containing group; l/Right enters a group or its first visible child. Enter opens an agent, or folds a selected heading. The number keys 1 to 9 (`list.group_1` to `list.group_9`) select the first agent of that numbered top-level group, or its folded line. Escape closes an opened question's options and never stops an agent. The existing full-screen child split remains available outside the list.

From the composer, the prefix followed by n/p opens the next/previous root in the same project, through the same switch as a click, opening the list if it is closed. The project is the one each row already carries (`AgentView.project`, the folder's name); helpers use their root's. The order is the list's, across every heading: saved groups and state headings only order the project's roots, so a working root and an idle one are neighbours (until 3 November 2026 n/p stayed inside the current root's section, and a project split by state headings skipped its other roots). NEEDS YOU is skipped, since it repeats rows listed below it, and so is Earlier: n/p never reopens a closed session, and an idle one opened from Earlier is live again while a terminal has it open. At either end, stop at the first/last root. If the project contains only the current root, leave the view alone. Prefix j/k open the next/previous project's first root and Shift-1..9 the Nth project's, by the same order, a project's place being its first root's; 1..9 open the Nth root of this project. The prefix is backtick or Ctrl-B, armed only while the draft is empty, so a backtick among words is typed and Ctrl-B moves left there; the prefix pressed again sends its own key (5 November 2026, tmux and Herdr habits). Escape cancels an armed prefix; a key that has no prefix action is discarded with the available actions shown, rather than becoming an accidental send or edit. The prefix consumes one following action and then disarms. Prefix n/p leaves keyboard input in the composer; prefix h, or F2 opening the list, explicitly enters list navigation, and prefix l leaves it.

## Short titles and the last reply

Agent instructions ask for a headline of five words or fewer at the start and whenever the goal changes. Keep this to a few lines in the shared profile text, within flow 28's 12,000-token prefix cap. A headline is human wording; no runtime truncation to five words and no model request are added.

The record summary already folds each `Body::Text` to set `spoke_at`. In that same pass, retain a bounded first sentence of the latest reply and expose it as defaulted `AgentView.last_reply`. Collapse whitespace; stop at the first sentence-ending punctuation followed by whitespace or the end, and cap at 512 UTF-8 bytes on a character boundary. A reply with no such punctuation uses its bounded text. This is a display excerpt, so abbreviations may end it early. An idle row shows this excerpt instead of "waiting for you"; with no reply yet, use the headline, then "No reply yet". Questions, interruptions and errors retain their existing priority. No per-row transcript fetches or model calls are needed. Replaying the record after restart reconstructs the same excerpt, and peer status streams carry it normally.

## Configurable terminal keys

One table per context under `[ui.keys]` maps plain action names to arrays of key strings, for example `[ui.keys.global]` with `prefix = ["Ctrl-A"]`. Missing names inherit shipped defaults; `[]` unbinds an action. `[ui.keys.global] prefix = ["`", "Ctrl-B"]` is the default. People and their agents edit the same plain TOML file. The live configuration stream carries changes to the terminal, where bindings compile only when `ui.keys` changes, rather than parsing strings on every keypress. `/keys` lists every action, its context and all effective keys, including unbound actions and the prefix.

The shipped `terminal/data/keys.toml` catalogue owns names, contexts, default chords and plain descriptions. It is loaded through `include_str!` and parsed with the same parser as personal overrides. Keys use `Ctrl-`, `Alt-`, `Shift-`, `Super-`, named keys such as `Enter`, `Space`, `Left`, `PageUp` and `F2`, or one printable character. The protocol carries only a defaulted nested map of contexts and actions to key strings. The terminal parses that map into native key events once per settings change. Invalid names, invalid chords and duplicate chords for different actions within the same context produce a config error in the terminal and leave its last valid compiled bindings in place. Separate contexts may reuse keys, as Enter already means different things in the composer and a picker. Bindings are bounded by the fixed action catalogue and at most eight keys per action.

Every terminal keyboard command goes through the catalogue: quit/stop, list and project navigation, prefix, status, questions and option numbers, child navigation, transcript paging and expansion, copy/cut/quote, attachments, model/effort pickers, voice, emoji, completion and shortcodes, all editor movement, selection, deletion, newline, undo and redo, and modal movement/accept/cancel. Ordinary printable text and bracketed paste remain input. The current modal and question precedence stays explicit; a key has an action only when that action is available in the active context. Existing defaults remain except for the list's approved j/k, h/l and Space behavior. On-screen hints use effective bindings; no hard-coded F2, Ctrl-S or Enter hint survives a remap.

The prefix acts only if the draft is still empty at its second key, and an action named in full under it runs only where its own context would take it; a paste or a quote into the draft ends browsing, as typing does. The person's editor runs from `$VISUAL`'s words as `shlex` splits them, one argv deciding both the program and vi's `-R`; `shlex` is already built for `cc`, so it adds no crate, and it replaces a shell whose own word splitting the program check could not see.

A helper that is done without a report also leaves the list, matching the card’s “stopped by its parent” state. A stopped child’s durable report stays in its parent record, but is cleared from its displayed row. Remote lifecycle facts remain on the parent’s server; a finished remote helper with branch work remains listed there, while its owner’s list lacks those parent facts.

A finished helper whose report says its branch has no commits beyond its base leaves the list as soon as the report reaches its parent. Helpers without a branch follow the same rule. Continuing clears the report and returns the helper to the list; only settlement ends continuation. This uses the parent summary’s recorded report, with no git or file reads while resolving the layout.

## Internal surface and data flow

```rust
// Existing protocol views: add no per-session group path.
/// Stable ids and at most three levels; children follow their root's matches.
pub struct Group { /* existing fields, including groups: Vec<Group> */ }
/// Flat depth-first sections; depth 0–2 is independent of agent parent depth.
pub struct Section { /* existing fields */ }
/// Defaulted, bounded display excerpt reconstructed from the record.
pub struct AgentView { /* existing fields */, pub last_reply: String }
/// Live key overrides, checked against the single action catalogue.
pub struct Ui { /* existing fields */, pub keys: BTreeMap<String, BTreeMap<String, Vec<String>>> }

// terminal/agents.rs, within the existing private terminal module.
/// The list's rows below its header, from the server-resolved sections.
pub(super) fn rows(app: &App, width: u16, now: u64) -> Vec<status::Row>;
/// Consumes semantic list actions; only Enter opens, through `press`.
pub(super) fn key(app: &mut App, key: KeyEvent);
/// Explicitly opens an agent or folds a heading through the existing action path.
pub(super) fn press(app: &mut App, action: Action);
```

1. The shared owner's definitions remain the only source of group names, rules and nesting.
2. API validation and resolution walk the same three-level shape; each session is placed once.
3. A layout streams ordered sections; each section contains only its own roots and helpers.
4. The terminal counts roots and helpers from row depths and hides folded subtrees, and builds each group's census once per draw.
5. One row builder draws a group's frame, headings, agents and helpers and their fold hits; the overview shares its frame, tabs, census, bars and clock.
6. Resident rows join the same visual grammar while retaining their existing server metadata and home-session targets.
7. The record summary supplies the latest reply excerpt in every agent row; idle rendering uses it directly.
8. Enter, a click or prefix n/p calls the existing guarded focus switch; composer input always targets focus.
9. Changed key settings compile the single action table; translation returns the key, context and action explicitly, and active contexts choose actions and the same table supplies hints and `/keys`.

The redesign's hard decision was to give preview its own displayed-session id, so that navigating never changed the recipient or draft. With previews removed that id is gone too: the displayed session is always the addressed one.

## Checks and evidence

Flow 56 passes shared three-level placement, refusal of a fourth level and duplicate ids, `/group`, versioned external editing, paired preview, idle excerpts, folding a complete group and a live Ctrl-Y status remap. Flow 20 checks “closing after this turn” in the working line and agent list.

Modified deletion, movement and selection chords are in the key catalogue. Empty-option questions retain `/answer`, numbered hints stop at nine, and scheduled threads omit the `/done` closing notice. The key contexts use the plain names documented in Configuration, quit hints use effective bindings, and unchanged key settings never recompile.

## Layout server identity

Layouts need only server identity and the configured resident home ids, so they skip
calendars and load. One private `identity(app, config) -> Server` derives id, label,
machine and realm for both layouts and full status; `residents::Config::homes()` adds
each configured home id with empty upcoming wakes and past runs. Status still retains
the full snapshot: load, voice capabilities, upcoming wakes and past runs. A configured
home with no agent row never appears.
