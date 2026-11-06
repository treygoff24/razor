# Templates

## Prompt skeleton (every helper)

Fill the brackets; keep the OUTPUT block verbatim.

```
You are reviewing a content package: [what it is, who reads it, what it's for].
The package: [absolute path]: [file inventory in one line].
Authoritative references: [absolute paths to source-of-truth documents].
NOTE: your folder may be a copy of the package, so these absolute paths may not
resolve. Re-resolve them relative to your own folder before concluding
anything is missing.
You are read-only: change no file. Your final report is the whole deliverable.

TRIPWIRES (any violation is a ship-blocker):
[frozen claims / forbidden phrases / numbers that must not drift, verbatim,
each with its intent]

YOUR DISCIPLINE: [the discipline, e.g. forensic accountant]. Do what it does:
[the method, e.g. reconcile every figure in the package to its source table];
with [the instruments, e.g. a grep for every number, a read of the cited
record]. Return [the evidence form, e.g. each figure that fails
reconciliation, with both values and their locations].
AXES: [3 to 5 axes from AXES.md, adapted to this package]

OUTPUT: findings only, ranked most severe first. For each finding:
[SHIP-BLOCKER|MAJOR|MINOR] · file · the offending text (short quote) · why it
is wrong · suggested fix. If a category is clean, say so in one line. Do not
rewrite the files; do not pad; no praise section.
```

## Starting the helpers

One `agent` call per helper, all in one response, then one `wait`, which covers every helper. A reviewer's `placement: worktree` branches from your last commit and cannot see uncommitted edits: commit the package first, or use `placement: in_place` with the read-only line above. Pick `model` from the family that did not write the package. The tripwires, references and OUTPUT block are the same in every brief: write them once in a file and pass it as `shared_brief`, so each brief holds only its discipline.

```
agent {action: start, name: review-fidelity, kind: reviewer, model: sol, effort: xhigh, placement: in_place, shared_brief: <path>, brief: <fidelity brief>}
agent {action: start, name: review-coldread, kind: reviewer, model: astra, effort: high, placement: in_place, shared_brief: <path>, brief: <cold-read brief>}
agent {action: wait}
```

Each helper's `agent finish` report holds its findings. Settle a reviewer started in a worktree with `decision: discard`: it has nothing to merge.

## Loop (review, fix, re-review)

A critical package (legal, money, security) loops: helpers in parallel per round, the coordinator fixes between rounds, until a round returns zero SHIP-BLOCKER and zero MAJOR. Start fresh helpers each round so no one grades its own earlier finding, and carry the previous round's disposition note into the next briefs.

## Disposition note

One file beside the package, `review-disposition-YYYY-MM-DD.md`:

```
# Adversarial review disposition: [date]

[One line: helpers, models and efforts, where the full findings live.]

Applied: [each applied finding, compressed: what changed and where.]

Declined, with reasons: [each declined finding and the written reason:
audience, register, deliberate design call. This section existing is the point:
a reader can audit what the reviewers said versus what shipped.]

[Tripwire audits: clean or failures. Mechanical invariants re-verified: which, result.]
```
