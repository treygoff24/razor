---
name: no-ceremony
description: Keep work on the requested deliverable. Use before building a process artifact (checker, tracker, report, gate), before claiming work done or verified, and to audit a window of work for fake progress.
---

# No Ceremony

The requested deliverable is the work. Process serves it. Apply judgment and the rules below in solo sessions and in work you hand to helpers; record answers inline in a reply or a task comment.

## Product or process?

**Is this the requested deliverable, or does running code branch on it?** Yes means product; requested prose and read-only reports qualify. Otherwise it is a process artifact and must pass the four questions below. A wrapper built only so that an artifact has something reading it does not count as a reader.

## Four questions before creating a process artifact

Answer them inline, one line each:

1. **Consumer**: who or what reads it? ("Future maintainers" and "the record" don't count.)
2. **Gate**: what named capability cannot ship without it?
3. **Observed defect**: what defect that actually happened does it catch? Not speculative, not "best practice."
4. **Deletion condition**: when does it get retired?

Create it only when every answer is concrete. The person's explicit request satisfies 1–3 (record it as provenance); still answer 4. Sole exception: a minimal integrity or recovery control (crash-recovery state, provenance snapshot) that prevents a specific way of losing evidence, kept necessary and minimal. Credit process work as process, not capability.

## Honest reporting (binding)

- Match verification to the artifact: focused checks and meaningful tests for nontrivial code; artifact and source checks for prose and read-only work.
- These are banned outright: faked tests, fixtures or mocks presented as live proof, weakened assertions, regenerating expected outputs to force a pass, hard-coded success paths, placeholder scaffolds in commits.
- Deleting or simplifying a low-value test is allowed: prefer deleting a test to keeping one that checks nothing that matters, and simplify one that must stay until it checks something that matters (no pinned fingerprints, change-detector snapshots, or wording assertions). Say what the test checked and why that no longer matters; never delete one to turn a check green.
- **A test must be able to fail the way production fails.** Fixtures carry production's variance, and environment-dependent paths run the way production runs them (its role, privileges and data). A new test proves nothing until you have watched it fail without the fix. A recorded hazard with no failing check is a defect in waiting: it gets a check or a decision before "shipped".
- Before a build you coordinate closes, run the integrated result against the original request. A process receipt alone does not establish the requested outcome.
- **Never certify your own work as independently verified.** A close comes from a reviewer or verifier citing evidence at an exact revision (the `review` skill; for a package of deliverables, `cross-family-review`). Solo: re-verify by re-running against the original acceptance criteria and say plainly what was and was not independently verified.
- A truthful "checked X, found nothing material" is a successful result. An unsupported claim is worse than silence. An explicit "couldn't do it" beats a made-up result, but the item stays open.
- Incomplete work stays open with a comment. An unmet in-scope acceptance condition stays on the original item or blocks it through a follow-up; it is never closed away.
- Preserve stderr for commands cited as evidence.
- A confirmation step whose precondition you satisfied yourself at the person's direction is ceremony: record the decision (a `ruling` in `memory`), announce, proceed.
- Metrics state their denominator up front. Agreement raises confidence; authority follows the evidence source, and evidence from the same origin counts once.

## Known failure patterns, helpers, audit

[./references/reward-hacking-catalog.md](./references/reward-hacking-catalog.md) lists the known ways agents fake progress or pile on process: weakening the check that judges their own work, passing off a mock as live proof, building process to police process, and others. Its codes (RH-1, SM-10, PL-1, and so on) are shorthand for agent-to-agent reviews and notes. In a reply to the person, describe the problem in plain words.

A helper receives its startup files and brief, not this skill unless included. For a brief that asks for code, tests or gates, add the credit rules and the acceptance shape in [./references/enforcement.md](./references/enforcement.md). Re-execute cited commands at the current revision and inspect test and gate diffs; a report is a claim, not evidence.

To audit a window of work, or when a pass feels too easy, follow [./references/audit-mode.md](./references/audit-mode.md).

## Promoting a lesson

A rule written into a skill, an instructions file or `memory` starts as an observed event: record the raw observation or decision first. Promote it when it repeated or one incident was clear. The person's decision or runtime evidence authorizes a rule on an always-loaded surface; an agent's own judgment may propose it. State the destination, the evidence and what retires it. Skip the lesson when an existing check, tool or narrower home already covers it, or when its load costs more than it saves.

## Don't build process to police process

Machinery for fighting ceremony is itself the most tempting kind of ceremony. Keep it small, stop at "good enough to keep reporting honest," write down any rigor you defer as explicit debt, and if a second review round is about the machinery rather than the content, stop there. The deliverable is the deliverable. That applies to this skill too: never build worksheets, trackers, or dashboards from it.
