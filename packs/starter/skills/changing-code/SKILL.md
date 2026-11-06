---
name: changing-code
description: Changing code. Before any edit, load the procedure for the moment (fix, add or design, structural review, challenge a subsystem); holds the standard every change meets and the design vocabulary.
---

# Changing code

Every change preserves or improves the best known realization of what the product promises today. Existing code is evidence of past decisions, not authority over this one.

## Load the procedure for the moment

| Moment | Load |
|---|---|
| Fixing or diagnosing anything: a bug, regression, flaky test, race, leak, wrong state | `repair` |
| Adding or replacing behavior, designing a module or a slice, or a change that would push a subsystem past its size budget | `task-design` |
| Reviewing a diff, branch or module for structure, or restructuring code with behavior preserved | `structural-review` |
| Questioning whether a subsystem should exist in its current form, or the same area breaking again with signs of one structural cause | `challenge-the-design` |
| Retiring a feature or deleting a mechanism | the deletion pass in `challenge-the-design`, on its own |

Load it before the first edit, with the skill tool. A one-line typo fix is the exception; everything that changes behavior or structure goes through its procedure. A review for what a change breaks or misses (correctness) is the `review` skill.

## Keep six things apart

Product intent, approved obligations, the current implementation, measurements, hypotheses, and the person's decisions. Confidence in prose never converts one into another: a measurement stays a measurement, a hypothesis stays a hypothesis until an experiment confirms it, and only the person turns a proposal into a commitment.

## Name the obligation first

Before changing anything, name the obligation the change serves: a line of the spec, a rule, a user-visible behavior, a test. The obligation sets the smallest change that is sound; the code around it is context. A change with no nameable obligation is a guess, and a guess is where to stop and look again.

## Vocabulary

Use these words exactly; every procedure and reviewer shares them.

- **Module**: anything with an interface and an implementation, at any scale: a function, a type, a package, a slice that spans tiers.
- **Interface**: everything a caller must know to use the module correctly: signature, invariants, ordering, error modes, configuration, performance.
- **Seam**: where a module's interface lives; the place behavior can be changed without editing there. A seam earns its place when behavior actually varies across it, a test substitute included; a seam with one adapter is hypothetical. A module may also have internal seams for its own tests; those are not part of its interface.
- **Adapter**: a concrete thing satisfying an interface at a seam.
- **Depth**: behavior per unit of interface a caller must learn. Deep is a small interface over a lot of behavior; shallow is an interface nearly as complex as what it hides.
- **Leverage** is what callers get from depth; **locality** is what maintainers get: change, bugs, knowledge and verification concentrate in one place.
- **The deletion test**: imagine deleting the module. If its complexity vanishes, it was a pass-through; if the complexity reappears across its callers, it was earning its keep.
- **The interface is the test surface**: tests cross the same seam callers do and assert observable outcomes, so they survive internal refactors. A test that must reach past the interface says the module is the wrong shape.

## Mechanisms that need a reason

A change that adds any of these is making a design decision, whether or not it was asked to:

- a new flag, mode, or boolean threaded through existing control flow;
- a retry, cache, fallback, or compatibility path;
- a special case added to a shared path;
- a second place the same fact lives;
- feature logic in a general-purpose module;
- a wrapper that adds no behavior;
- a file crossing 1000 lines (or the project's own limit).

For each one, name the obligation it serves and compare it with the simpler alternative: fixing the fact at its source, one authoritative lifecycle instead of a second flag, deleting the mechanism it works around. A permanent mechanism earns its place through that comparison (a bounded retry can implement an availability promise; a cache can implement an approved latency requirement), and the report says so. A temporary compensation names its retirement condition and who decides. One that is neither stops the change until its procedure has been run. This is the one rule for these mechanisms; `repair` and `structural-review` apply it rather than restate it.

## Report in proportion

A typo gets a line. A fix names the obligation, the cause, the change, and what ran. A design change adds the alternatives considered and what remains unverified. Verification and honest reporting belong to `no-ceremony`; this file adds only the obligation and the mechanisms above.
