# Lane briefs

Skeletons for the four lane kinds a campaign sends. The orchestrator writes each one into `perf/<date>-hill-climb/briefs/<kind>-NNN-<slug>.md` and passes it as the `brief` of an `agent` `action: start`, which is the lane's whole task. Measurement and scout lanes start `placement: in_place` with `cwd` the campaign worktree; change lanes start `placement: worktree`; review lanes start with `kind: reviewer` and a model from the other family than the author's. Every brief opens with the same three lines, so a lane never has to infer its standing:

> Pre-approved <kind> lane for the <project> performance hill climb. Read perf/<date>-hill-climb/BRIEF.md first; it is the contract and its boundaries hold. You are not alone: preserve others' edits, never revert work you did not write, and start no helpers of your own.

Then the parts below. A lane that knows what it decides alone and what it stops for rarely idles a slot asking.

## Measurement lane (phase 1, read-only)

- **Area:** <processes | CPU and RAM | disk | latency | journeys>, and the question it answers.
- **Owns:** `perf/<date>-hill-climb/baseline/<area>.md`, `scripts/<area>*.py`, and `<project>-perf-data/<date>/baseline/<area>*`. Everything else is read-only.
- **Rules:** no source edits, no compiles, no commits. Read the live system's logs and `/proc` directly; never call a command that writes live state; never signal or attach to a live process. Every sample records UTC time, 1-minute load and whether a compile was running; historical logs say "load unknown", never today's load. Keep no secrets, raw tool inputs or environment values, only timing and process metadata with enough path-and-line references to audit. Check the real event shapes before assuming the brief's examples. A missing measurement is "unverified", never zero.
- **Returns:** a ranked table (target, cost, frequency, aggregate cost, evidence path and lines, likely code path, what is still unknown), raw JSON in the data folder, and a short Markdown report. Sanity-check the counting and arithmetic, and say what the observer itself cost.

## Scout lane (read-only)

- **Target:** <the ranked target and its baseline numbers>, stated as observed cost, not proven overhead.
- **Owns:** `perf/<date>-hill-climb/baseline/<target>-scout.md` only. No code, builds, tests, benchmarks or profiler attachment.
- **Returns:** at most three source-backed candidate changes ranked by expected removable cost × frequency, each with file and function references, the smallest private scenario that would measure it, the targeted test files, and whether it needs anything only <person> can approve. Separate hypothesis from evidence.

## Change lane (one experiment)

- **Worktree:** your own, on its own branch from `<campaign commit>`. Work only there.
- **Target and evidence:** <the target, its baseline numbers and conditions, prior work not to redo>.
- **Owns:** <source paths>, their tests, `experiments/NNN-<slug>.md`, `scripts/<slug>-*`, and `<project>-perf-data/<date>/experiments/NNN-<slug>/`. Other source is read-only; report a needed boundary expansion instead of taking it.
- **Do:** profile the path under a realistic private workload, choose one change the data supports, and build private A and B from the same base with the same flags, keeping both with their hashes. Run at least <five> paired runs with conditions per sample and check that both sides did the same work and gave the same results. A microbenchmark alone cannot certify a product saving. Never use an installed copy as A.
- **Compiles:** you are compile lane <1 of 2>; every build goes through <the shared build slot>, at most <N> jobs, and never while measuring.
- **Checks:** <the targeted test commands for the touched code>, lint and format for those crates or packages. Red-prove each new test once. Never weaken an existing test. No full gate.
- **Stops for:** a new dependency, a format, wire or auth change, a deleted test, a visible behavior change, a redesign; write a proposal instead. If a sandbox stops the product from running, hand the orchestrator the exact runnable commands and keep profiling.
- **Returns:** the write-up from the experiment template with a kept-or-reverted recommendation and the rerun command, as your `agent finish` report. Commit the source change alone first (subject at most 72 characters; body with before, after, method and run count), then the write-up in a second commit; the orchestrator repeats the A/B and decides. If nothing is supported, commit only the write-up, keep the data, and name the next measured cause.

## Review lane (read-only, another model family)

- **Reviews:** the diff of branch `<lane branch>` against `<base>`, and the experiment's measurement plan and scripts.
- **Hunts:** reachable behavior changes (ordering, cache invalidation, stale or missing results, lifetimes, error paths), whether existing tests exercise the change, and ways the measurement could produce a false positive (different work on the two sides, contaminated pairs, an unvalidated instrument, setup outside the measured window).
- **Rules:** no edits, compiles, test runs, commits or live calls. Checks still running are unverified, not defects. Do not accept savings before results exist.
- **Owns:** `experiments/NNN-<slug>/review.md` only. Severity-ordered findings with path, line and the reachable case; verified defects apart from residual risks; the diff hash reviewed. If there is nothing, say so plainly with its limits. Report once and stop.
