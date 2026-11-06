---
name: challenge-the-design
description: Challenge whether a subsystem should exist in its current form: redesign it blind, compare honestly, audit its mechanisms. Use when one area keeps breaking, a budget must grow, or a design looks dated
---

# Challenge the design

The incumbent architecture has one unearned advantage: it exists, so every agent reads it first and reasons from it. This procedure removes that advantage for one subsystem, under controls honest enough that "the existing design is excellent, change nothing" is a real and welcome result. It is rare: run it when evidence points at a subsystem, not as routine hygiene. The vocabulary (module, interface, seam, depth, the deletion test) is in `changing-code`.

## 1. Choose the target from evidence

If the person named the subsystem, take it. Otherwise read the evidence: the areas that keep appearing in `git log` and in recent fixes; where understanding one concept means bouncing between several modules; modules whose interface is nearly as complex as what it hides; a size budget under pressure. Co-change in git is evidence of coordination, never proof of bad design; recurring fixes in one area are a hypothesis about its model, not a verdict. Apply the deletion test to anything that looks like a pass-through.

Read the project's design documents and glossary for the area before writing anything, so the brief in step 2 uses the project's own words for its concepts.

**Complete when:** one subsystem is named, with the evidence that chose it and the question being asked of it (replace, simplify, retire, or confirm).

## 2. Write the behavior-only brief

The brief carries what the subsystem must do and nothing about how it does it: the approved behaviors, operating constraints, compatibility obligations that genuinely bind, and real workload evidence (sizes, rates, latencies). It names no files, modules, types or rationale from the incumbent.

Then read it back as the challenger would: feature wording smuggles incumbent assumptions ("the reconnect queue drains…" presumes a queue), and a brief written from the incumbent's design pages smuggles its rules as behaviors (a bookkeeping entry nobody can observe; a rule one implementation needed). Rewrite each such line as an outcome, cite the product's own spec rather than the design pages wherever both exist, and tell the challenger in the brief that it may question the formulation of any requirement while keeping every approved purpose. Expect to miss some: the comparison in step 4 lists the obligations that turned out to be formulations, and they are discounted there.

**Complete when:** the brief reads as outcomes and constraints only, and a reader could not reconstruct the incumbent's structure from it.

## 3. Design blind and freeze

Give the challenger only the brief, in a workspace that holds none of the incumbent's source, history, or design rationale until the freeze: a helper (`agent` with `action: start`, `placement: in_place`) whose `cwd` is a fresh folder outside the repository holding only the brief, told what else it may read. Record what the challenger was given and what it could reach. It returns a design: the interface, the state model and where each fact lives, what the implementation hides, the dependencies and their adapters, the common caller's code including setup, ordering and failure handling, and predictions (size, hot paths, failure modes, what will be hard to change).

For an interface question rather than a whole-subsystem challenge, run the design two or three times, each helper in its own fresh context, under different pressures (minimize the interface to one to three entry points; make the most common caller trivial; ports and adapters at every external seam). A first design is rarely the best, and causally different designs compare better than three stylings of one.

Freeze before any reveal: write the design to a file (for a read-only challenge, into the report itself), record its SHA-256 (`sha256sum`) and the time, and only then open the incumbent to the challenger. A challenger that had seen the incumbent before the freeze yields an informed comparison, and the report labels it so.

**Complete when:** the frozen design's hash and time are recorded with the inputs the challenger had, and no incumbent source, history or rationale reached it before that moment.

## 4. Reveal and compare

Show the incumbent code, history, design docs and measured costs to a comparator helper (`kind: reviewer`) from the other model family than the challenger's, with the brief, the frozen designs and the deletion pass ([the comparator's brief](./references/comparator-brief.md) is the shape that worked). It first lists which obligations in the brief were incumbent formulations rather than behavior, then compares all candidates against the same obligations and on the same axes: obligations met and missed; depth, locality and seam placement; the concepts and coordination a reader must hold; size; runtime cost under the workload evidence; debugging cost; how each absorbs the changes actually coming. Every claim carries its evidence tier: measured or exercised in the repository; reasoned from source; or the design's own prose. Name what the comparison reveals in each direction: accidental residue in the incumbent, defects in it that the brief exposes, obligations the challenger missed, legitimate differences that are tradeoffs rather than defects.

No automatic preference for the challenger, the newer code, fewer lines, or the better explanation. A design is evidence; measured behavior is better evidence; prose about either is weakest. When the verdict turns on runtime cost or on how each absorbs change, put both candidates through the same workload or the same change task under the same evaluation budget, and state any unequal condition. When the challenger found a new conceptual model, give the incumbent's side that model before attributing the rest of the advantage to construction. Implementing the challenger in an isolated worktree, to test both against the same obligations and migration requirements, happens within an authorized experiment scope; otherwise the report names the experiment and the decision it would settle.

**Complete when:** each axis has an answer for both candidates with its evidence tier marked, and every claim the recommendation rests on has discriminating evidence or stands as a named open question.

## 5. The deletion pass

This step also runs alone, for retiring a feature or deleting a mechanism. For every mechanism in the subsystem, name the promise that still depends on it. A mechanism with no live promise is a candidate for retirement; a mechanism whose dependents are unknown is a candidate for investigation, never for deletion. Moving complexity into a generated layer or a tool is not deleting it: count the independent maintenance decisions that remain.

**Complete when:** each mechanism is tagged with its live promise, "none found", or "unknown".

## 6. Report for the decision

A ranked list: keep, simplify, replace, retire, or investigate, each with its evidence and what it would cost. "The existing design is excellent; change nothing" ranks like any other result. Anything that changes a product promise, an architectural commitment or a size budget is the person's decision, and the report says exactly which decision is outstanding. Walking one chosen candidate through its constraints and consequences with the person is what `grilling` is for.

**Complete when:** the report carries the frozen design's hash, the comparison, the deletion pass, the ranked list, and the decisions that are the person's.
