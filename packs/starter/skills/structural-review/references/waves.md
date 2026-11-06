# Executing a refactor backlog in parallel waves

The execution half of `structural-review`. Read it when a review or a refactor backlog has produced a large pile of behavior-preserving findings.

Doing them serially is slow; fanning out naively means two helpers edit the same file and clobber each other. The safe way to parallelize: **dependency-ordered waves, each wave a set of helpers with strictly disjoint file ownership, checked per wave by the coordinator (you) and gated once at the end, with the highest-drift-risk waves verified by a fresh adversarial reviewer.**

This is **execution**, not diagnosis: it assumes the findings exist and are behavior-preserving.

## When to use

- **Use** for about ten or more mechanical findings across many files; a codemod, deduplication or single-source sweep; decomposing several oversized files; any large behavior-preserving change whose edits partition by file.
- **Skip** it for a handful of fixes (just do them); anything that changes behavior, public signatures, or on-disk or wire contracts (those need the person's review); a single tangled file only one owner can edit.

## The loop

For each wave: **partition → fan out (disjoint owners) → check → commit → (riskiest waves) adversarial verify**. Start the next wave only after the previous one is committed and its checks are green; later waves often depend on earlier waves' central changes.

## 1. Partition by disjoint file ownership

The one invariant that makes parallel editing safe: **no two concurrent helpers touch the same file.**

- Partition findings **by file, not by category.** Each helper gets an explicit `owns` list (disjoint from every sibling's) plus the exact finding IDs scoped to those files. Hand it the IDs, not "go find issues"; narrow scope keeps it out of a neighbor's file.
- A finding that needs a file another helper owns is a **collision**: give both findings' files to one helper, or hold the cross-cutting finding for a later single-owner wave. Helpers cannot coordinate on a shared file; they will race.
- When a fix needs a helper function in a *shared* central file (say a new accessor three call sites will use), that central edit is its own earlier wave or has a single owner; the call-site updates fan out *after* it lands.
- Start one helper per partition (`agent` with `action: start`, `placement: worktree`, so each works on its own branch cut from your last commit). Each brief carries its `owns`, its `ids` and the guardrails below; the helper edits only owned files and reports what it changed. Merge a passing helper with `settle`, `decision: merge`. Commit the merged wave before starting the next, so the next wave's worktrees begin from it.

## 2. Order waves by dependency

- **Central changes before the consumers fan out.** Single-source helpers land before the call sites that use them; an extracted module lands before its importers update.
- **Deduplicate before decomposing.** Collapse duplication first, *then* split the giant file, so you split the deduplicated version rather than two copies.
- **Replacements before deletions.** Never leave a wave's commit where the replacement does not exist yet: it will not build and it breaks bisect.
- An ordering that has worked: dead-code removal → single-sourcing of shared facts and contracts → merge and extraction → per-unit deduplication → giant-file decomposition.

## 3. Pure move behind a facade (decomposing a giant file)

Decomposing an oversized file is a *pure move*: zero behavior change, identical public surface.

- Keep a **facade** at the original path that re-exports the moved items, so external callers' paths stay valid. Diff the set of public symbols before and after.
- Move code **verbatim**: no renames, no signature changes, no reordered match arms, no altered string literals (many feed SQL, wire or on-disk contracts). Move each item with its tests.
- **Brief helpers on the visibility seams; this is where splits break.** Private fields, methods that were private-by-file and now cross a module boundary, items that need `pub(super)` or `pub(crate)` widening (never wider). Name the specific seams up front: which fields are private, which helpers cross the new boundary.
- Deduplication folded into a move (one helper replacing three near-identical copies, one SQL constant replacing six repeats) is **not** a pure move: call it out and prove the generated output (the SQL string, say) is identical.

## 4. Checks: helpers run targeted tests, the coordinator gates once

- Helpers run the targeted tests for the files they touched and their neighbours, with a cap on parallel test workers; N helpers each taking every core is how machines melt. The coordinator runs the narrow checks each wave needs (typecheck, formatter check, the touched packages' tests) as it merges.
- The **full project gate runs once, on the final integrated branch**. Keep that one run complete: a per-wave subset (lint and tests only) lets formatting, documentation builds, doctests and benchmark checks through, and they all surface together at the end. So add the formatter check and the documentation build to the wave checks, and run the canonical gate in full exactly once before declaring done. If a fix breaks something afterwards, re-run only the targeted tests that check the fix.
- Keep one build cache warm across the session so wave checks stay cheap. Fix compile and visibility stragglers yourself; giant-file splits most often produce them.
- Self-selected verification skews green: check at the whole-package or workspace level, never per edited file.

## 5. Adversarially verify the riskiest waves

A green gate proves it builds and the tests pass. It does **not** catch a flipped comparison in untested code, a dropped error check, or byte-level SQL drift inside a "moved" function. For pure-move and deduplication-heavy waves especially:

- Commit the wave, then hand the diff to a fresh read-only reviewer helper (`kind: reviewer`) from the other model family than the one that wrote the changes. Prompt it to *refute* the "pure move" claim: reconstruct deduplicated SQL byte for byte, diff each moved body against its pre-move form, confirm no `pub` leaks, check unsafe blocks moved verbatim.
- Tell it what the checks already proved (lint, format and tests green) so it spends its budget on what the gate cannot see. Ask for BLOCKER, RISK and NIT, and for "nothing real" said plainly rather than manufactured findings.

## 6. One wave, one commit; watch the staging trap

- One commit per wave, its checks green before committing, for a clean dependency-ordered history.
- **The staging trap:** `git commit` commits the *entire index*, not just what you just added. A pre-staged deletion or another change already in the index gets swept into the wrong commit, and a "pure formatting" commit silently ships a 2000-line deletion that will not build. Stage by name, then run `git diff --cached --stat` and compare the staged set with the wave's owned files before every commit. Unstage only your own stray paths, by name (`git restore --staged <path>`); staging you did not create belongs to someone else's work, so leave it and ask its owner. A wrong commit is corrected by a follow-up commit, never by amend or reset.

## 7. Defer behavior-changers and judgment calls

Behavior-preserving is what makes a wave parallelizable and safe. The moment a finding changes behavior, a public signature or a serialized shape, or needs a judgment call (two crash-safe-persistence copies have already diverged: which one wins?), it leaves the wave. Document the deferral in the commit message and the closeout, and surface it for a decision. Resist "just also do this one": that is how a safe sweep grows a behavior bug.

## 8. Do not let an environmental gate red-herring a clean refactor

A non-deterministic or performance gate can flag a pure-move wave for reasons unrelated to the diff. When a benchmark "regresses" on a pure-move wave, compare old and new under the same workload and environment (same seed; check whether untouched metrics also "regressed"; compare generated code where the toolchain makes that cheap), and report what the comparison establishes: an environmental cause (load on a saturated machine, a non-deterministic corpus, a stale baseline) or a real one. A baseline owned by a person is never edited to make a gate pass.

## Closeout

Run the full canonical gate once on the final integrated branch, then get one last adversarial pass on the whole diff. Report: waves landed, what each verified, the deferred items (with why), and any environmental-gate caveats, kept apart from real findings.
