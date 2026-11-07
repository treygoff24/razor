# Product

<!-- impeccable:product-schema 1 -->

Razor's terminal app, as design work needs it. Written on 6 November 2026 from SPEC.md, which governs where the two differ, from Trey's rulings, and from his answers to the product interview the same day, quoted where this file states them. The phone app is a separate project with its own product record.

## Platform

A terminal app: Rust on a character grid, drawn in 24-bit colour, on Linux (x86_64 or arm64) or macOS on Apple silicon. It is not web, iOS or Android. Design work is judged from rendered frames and screenshots at the sizes below, not from a browser.

## Users

Anyone who wants to put the subscriptions or API keys they already have to work through agents, from whatever screen is in front of them. Trey, 6 November 2026: "Razor is for anyone that wants to be able to use their Claude Code and Codex subscriptions or their API keys, use their agents from any surface, whether it's their iPhone, their iPad... Their Mac, a web browser, whatever, to get productive work done in a delightful, seamless, polished user experience."

Today that is Trey first, who runs many agents at once on a Mac and a Linux server and works in this terminal all day ("It is first a personal tool that I have found there is massive demand for"), then the people running their own Razor servers from the open-source release. The terminal and the iPhone app exist today; an iPad, Mac or web client is the direction, not a shipped surface (SPEC defers the web app).

## Product Purpose

Razor is an agent harness built for running a crew of agents rather than one. In SPEC's words, "the place where an agent does its best work, and the frame that lets one person run a crew of them. For the agent it is a good workshop: legible, trusting, continuous. For the person it is the multiplier."

The terminal app is how the person works with that crew at a desk: talk to one agent, see what every agent on every paired machine is doing, answer what they ask, open any one of them. Success is that the person always knows who needs them and gets there in a keystroke or two.

## Positioning

"The Omarchy Linux kernel of harnesses": an opinionated distribution, the harness version of an opinionated Linux distribution, over borrowed runtimes (Anthropic's Agent SDK, OpenAI's Responses API), so the subscriptions a person already has are enough. Its first name was the ultimate harness. Trey, 6 November 2026: "we took what works and what the research says works for AI agents and modern LLMs from the research and from all of the other best open-source harnesses. And we combined and improved on and refined it all."

What a neighbouring harness could not truthfully copy, in Trey's words of the same day:

- "the seamless, polished, low-friction ability to orchestrate a fleet of agents who themselves can spawn their own sub-agents across multiple machines";
- "The ability for parent agents to spawn child agents on other servers";
- the server itself, "and everything that flows from that": agents that keep running after the terminal closes, one list of every agent on every machine, many clients on one crew;
- "The voice agent feature";
- "The animated, fun user interface".

## Operating Context

- Used all day at a desk, on screens from a 14-inch laptop to a studio display, and sometimes over SSH from a phone-sized terminal or inside tmux.
- Many agents run at once on several servers. A server can be unreachable; its agents stay listed.
- The person moves between three views of the same work: the session alone, the agents list beside a session, and the full-screen agents overview. "The point of this is to breathe calm command control." (Trey, 5 November 2026, on the overview.)
- Keyboard first; the mouse works too (clicks open and fold, the wheel scrolls).

## Capabilities and Constraints

- The terminal is a client of the Razor server and keeps no state of its own (SPEC section 5). Anything a screen shows comes from the server's API; a fact the server does not send yet is new server work, not a terminal invention.
- One state per agent: needs you, working, done, stuck.
- Size budgets in SPEC section 6 cap every part of the code, the terminal included.
- Speed is part of the product: a screen with nothing moving on it draws nothing; motion is paced so an idle terminal costs almost no CPU.
- Display settings the person controls: `[ui] motion` (off draws everything still), `[ui] blocks` (octant or half-block drawing; tmux and narrow screens get half blocks), `[ui] stage`, and `[brand] colours`, which replace theme colours by name.
- Screen widths: under 70 columns is a phone-sized terminal, 70 to 109 narrow, 110 and up wide; the side pane docks from 140 columns.
- Keys: Escape stops what the agent is doing; Ctrl-C is reserved for quitting. Left and Right step out and in between the three views (Trey, 5 November 2026: "left arrow key in a noraml session opens the existing agents view. left arrow key again opens this new full screen view").

## Brand Commitments

- "The existing look" (SPEC section 5): the terminal is built to the Loom cockpit Trey designed; DESIGN.md records it.
- The colour law: colour is state, never decoration, and never the only carrier of meaning.
- The name is Razor ("Occam's razor for the law of the rebuild, and a tool that takes the shape of the hand that holds it"). A deployment pack may rename it and recolour it through `[brand]`.
- The pixel creatures of the workshop scene, shared with the phone app, are the crew's faces.

## Evidence on Hand

- The approved agents overview and list prototypes of 6 November 2026, an interactive page with screenshots at 64 and 48 columns and on a 16-inch laptop, kept with the project's private design records.
- The captured Loom cockpit (66 frames with their colours, every glyph, every animation and its timing), also kept privately.
- No user research beyond Trey's own use, no outside testimonials, no usage numbers from other users. Do not invent them.

## Product Principles

1. The person knows at a glance who needs them; needs-you outranks everything else on every screen.
2. Every screen is a pleasure to look at. Trey, 6 November 2026: "every screen should be pleasant to look at and evoke joy, delight, fun", and "We are not afraid to take distinctive stances here in terms of aesthetic and user interface." Distinctive, not generic: a screen that could belong to any dashboard is not done.
3. Calm about state: nothing shouts unless its state changed; a quiet crew looks quiet, and the delight lives in the craft, not in alarms.
4. Every screen is a view of server facts, never a second source of them.
5. Fast and cheap at rest: drawing what changed, nothing more.
6. Delete before adding: "Anything of Loom's that went unused in his last thirty days of use is out unless he names it."

## Accessibility & Inclusion

- Every state has a mark and a word beside its colour, so nothing depends on telling colours apart.
- `[ui] motion` off gives every animation a still, settled form.
- Every layout holds down to a 40-column terminal, and drawing falls back to half blocks where octants are unsupported.
- Text colours keep strong contrast on their dark ground; DESIGN.md names each pairing.
