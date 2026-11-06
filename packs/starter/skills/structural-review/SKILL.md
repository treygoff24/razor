---
name: structural-review
description: Structural review of a diff, branch, module or repo: simplifications that delete concepts, flags on shared paths, thin wrappers, oversized files, misplaced logic; also parallel refactor waves.
---

# Structural review

Working code is the floor. This review asks whether the change left the codebase simpler to reason about than it found it, and where a reframing would delete a whole category of complexity rather than tidy it. Be ambitious about simplification and sparing with findings: a few high-conviction structural findings outrank a long list of nits. It sits beside the `review` skill, which judges correctness (what a change breaks or misses); this one judges structure.

## Preflight

Read the repo's instructions for the scope (`AGENTS.md`, `CLAUDE.md`, nested guidance) and inspect `git status --short --branch` and `git stash list`; unrelated dirty work stays untouched. A review request is read-only; when the request includes fixing, the review completes before any edit. Static probes (`rg`, file counts, import graphs, scoped lint or typecheck) are cheap and yours to run; heavy gates (full tests, builds, browser suites) run once, by whoever coordinates. For a whole-repo scope, split the work into read-only helper reviewers by concern (ownership and layering; oversized files; branching, flags and state models; type and boundary contracts; duplicate abstractions; async flow and atomicity; dead and temporary paths), each started with `kind: reviewer`.

**Complete when:** the scope, the mode (review-only or review-then-fix) and the tree's dirty state are known.

## What to look for

Apply every rule to every meaningful change or module in scope. The vocabulary (module, interface, seam, depth, locality, the deletion test) is in `changing-code`.

- **A reframing that deletes.** Could fewer concepts, branches, helpers or layers produce the same behavior? Reframe state so a conditional disappears; move ownership so the feature becomes natural; turn the special case into the default flow.
- **Moved is not deleted.** A refactor that relocates code without reducing what a reader must hold is churn. Count concepts before and after.
- **Mechanisms that need a reason.** A new flag, mode, retry, cache, fallback, compatibility path, or second copy of a fact passes by the rule in `changing-code`: the obligation it serves and the comparison with the simpler alternative, or a retirement condition with an owner. Without either, it is the finding.
- **Shared paths.** Feature-specific checks scattered through general-purpose code; conditionals bolted into unrelated flows; repeated conditionals signalling a missing model.
- **Wrappers and indirection.** Thin wrappers, identity abstractions, pass-through helpers, generic machinery hiding a simple data shape. Apply the deletion test.
- **Owning layer.** Logic belongs in the module that already owns the concept; a bespoke helper beside a canonical one is a duplicate.
- **Contracts.** Casts, `any`/`unknown`, optional parameters and ad-hoc object shapes that hide the real invariant where an explicit type would state it.
- **Size.** A file crossing 1000 lines (or the project's own limit) is a presumptive design smell; a change pushing one across is a presumptive blocker.
- **Flow.** Sequential orchestration of independent work; related state updated non-atomically so partial state is reachable.
- **Residue.** Dead paths, obsolete flags, "temporary" branches, deprecated APIs still wired.
- **Test surface.** Tests exercise the owning interface and assert observable outcomes. Flag tests coupled to private structure, and helpers tested in isolation while the defects live in how their callers use them.

## Findings

For each finding: the problem (what structure makes the code harder to reason about), evidence (files, functions, the pattern), the simplifying move (the smallest behavior-preserving reframing that deletes complexity), risk (what could break if done carelessly), and confidence. Order: structural regressions; missed simplifications; branching growth; contract and boundary problems; size and decomposition; legibility. Mark items that need the person's judgment instead of deciding them.

**Complete when:** every rule above has been applied to the scope, each finding carries all five parts, and the list is ordered.

## Verdict

Approve only when there is no clear structural regression, no obvious missed simplification, no unjustified growth in file size or branching, no abstraction that makes the code harder to reason about, no contract churn hiding the design, and no logic in the wrong layer when a canonical home is clear. Preserving incidental complexity when a plausible simplification would delete it is a presumptive blocker, as is adding ad-hoc branching that tangles an existing flow.

Review-only output: the verdict against that bar; the findings in order; items for the person's judgment; a suggested sequence for the fixes.

## When the request includes fixing

Fix in separate behavior-preserving passes after the findings are clear, high-confidence findings only unless the person approves an ambiguous restructuring. One file, one owner across concurrent helpers. A simplification that might change behavior stops and reports instead. The coordinator integrates, formats, and runs the gate once. For a large mechanical backlog, a cross-file deduplication sweep, or the decomposition of several oversized files, read [waves](./references/waves.md).

Review-plus-fix output adds: findings implemented; files changed by theme; what ran and its results; findings skipped and why; remaining risk.
