# <Project> performance hill climb (orchestrator brief)

<!-- Fill every <blank> from the real project, then delete this comment. Write it for a
reader who has never seen the project: name paths, commands and processes exactly.
Commit it on the campaign branch; the kickoff names that commit. -->

Written <date> by <who> for <person>'s orchestrator. This file is the whole contract; the kickoff points here. Where this brief and the repository's agent rules disagree, this brief wins, as "Which repo rules apply" says.

## Goal

Make <project> measurably faster and lighter at the things <its users> do all the time: <the five to eight everyday actions>. Find the hot spots with real data, fix them one measured change at a time, keep only what the numbers prove, and leave a ledger a reader can audit. <The person's own words for the goal, quoted.>

## Context you need

- **Repo:** `<project>-perf`, a worktree of `<project>` on branch `perf/hill-climb-<date>`, cut from `<base branch>` at `<commit>` (the last commit that passed the full gate). <Where the build is described; the architecture doc; earlier perf campaigns and what they already took, so nobody redoes them.>
- **The live system:** <what runs on this machine during the campaign, its process names, its state directory>. It is the real workload. Measure it; never touch it.
- **Timing data it already records:** <log paths, event kinds, the fields that give per-operation latency, startup timings, per-hook durations>. Check the real event shapes before trusting this list.
- **Instruments already in the repo:** <rigs, probes, benchmark scripts, status commands>. Use them before writing new ones.
- **Machine:** <cores, OS, what else builds here and how far the load swings>. Every sample records the 1-minute load and whether a compile was running, so no number is read without its conditions.

## Phase 1: baseline, before any change

Produce `perf/<date>-hill-climb/baseline.md` (prose and tables), with raw samples in `<project>-perf-data/<date>/baseline/` and the scripts that made them in `perf/<date>-hill-climb/scripts/`, covering, from the live run:

1. **Processes:** every process the system owns over time (count, kind, lifetime, parent), and bursts of short-lived processes per minute with the paths that spawn them.
2. **CPU and RAM:** per process kind, CPU seconds per minute and RSS, with the idle cost of each long-lived process. Totals with the load alongside.
3. **Disk:** state size and growth, the largest logs and databases, writes per second where measurable.
4. **Speed:** per operation, median and p95, the split between the system's own overhead and waiting on others, and the count. Then the journeys: <each everyday action, timed end to end with the rig or from the live logs>.

Rank the whole table by cost × frequency and name the top ten targets with the evidence line for each. That ranking is the hill.

## Phase 2: climb

- **One change per experiment,** each with `perf/<date>-hill-climb/experiments/NNN-<slug>.md`: hypothesis, the exact change, the A/B method, the numbers with their conditions and run count, the decision and why.
- **Metric:** <the metric that survives this machine: instruction counts (`perf stat -e instructions:u`), process CPU under recorded load, RSS, or wall time in paired runs>. Interleave A and B. A claim needs non-overlapping ranges, or at least five paired runs with a consistent sign. A before at load 5 against an after at load 30 is not a measurement.
- **Measure the product, not the test.** Use the private rig for A/B (private state, repeatable), and the live logs to confirm a kept win after install. You do not install; say what to look for.
- **Keep only proven wins.** A kept win is one source-only commit on your branch, explicit pathspecs, subject at most 72 characters, body giving before, after, method, run count and the targeted tests with their counts. A reverted experiment leaves its write-up and no code.
- **Correctness stays green.** For each change, run the targeted tests of the code you touched: <exact commands, through the project's test runner and build slot>. Red-prove each new test. The full gate is not yours.
- **Data stays out of the repository.** Raw samples, per-rep rows, profiles, logs, patches and builds go in `<project>-perf-data/<date>/`; commit prose, small scripts and summary tables only.
- **Update the ledger** `perf/<date>-hill-climb/LEDGER.md` after every experiment, in plain words.

## Sub-agents

You orchestrate; lanes do the measuring and the changes. Start each lane as a helper with the `agent` tool (`action: start`), and wait on all of them with `action: wait`. Models are `opus`, `sonnet`, `haiku`, `fable` (Claude) and `sol`, `terra`, `luna`, `astra` (GPT). Mix model families across lanes, at the effort each lane needs.

- Phase 1 lanes are read-only and share this worktree (`placement: in_place`), one per area. Each returns a ranked table with evidence.
- Phase 2 change lanes each get their own worktree and branch (`placement: worktree`), one lane per experiment, and commit the source change and the write-up as separate commits. You settle a kept win with `merge` after re-reading its numbers and repeating its A/B yourself. For a reverted one, cherry-pick its write-up commit onto the campaign branch, then settle with `discard`.
- At most <two> lanes compile at once, through <the shared build slot>. Measurement lanes never compile while sampling.
- Every experiment gets a read-only reviewer helper (`kind: reviewer`) from a different model family than its author.
- A lane brief states its owned files, read-only areas, what it decides alone, what it stops for, its artifact paths and its checks. Its report is a claim.

## Staying on track

- Work in this order: baseline, ranked hill, experiments from the top down.
- After each experiment, update the ledger, then pick the next target by expected cost × frequency saved, not by ease. Re-rank after every three kept wins.
- A measurement you cannot explain goes under Unexplained in the ledger; return to it only if it stays at the top of the hill.
- Decide from this brief rather than asking. What only <person> can decide goes in the ledger's "Needs you" section while you carry on with the rest.

## Boundaries

- **Never write to the live system:** nothing under <state paths, config paths, the main checkout and its other worktrees>; no commands that write live state. Reads are fine.
- **Stop only what you started:** no `pkill`, `killall`, `kill -1`, or stopping units you did not create. Stop your own processes by PID or by the unit you started them in.
- **Never** run the full gate (<its commands>), the landing script, or a push to <protected branches>. Commits on your branch are fine.
- **Never generate artificial load** to test behaviour under load. Measure under natural load and record it.
- **Stop and write a proposal** instead of making a change that needs a new dependency, an on-disk or wire format change, an auth change, a deleted test, or a design change.
- <The project's hard constraints a speedup must not route around.>

## Which repo rules apply

<Which parts of the repository's agent rules (`AGENTS.md`, `CLAUDE.md`) bind this session and which do not; which files are read-only for it (state files, issue trackers, journals). The campaign folder is its record.>

## Done when

All of these are true, and the final message quotes them:

1. `baseline.md` exists with the ranked hill (top ten, each with cost, frequency and evidence).
2. Every top-ten target has an experiment write-up, kept or reverted.
3. Each kept win is one commit on `perf/hill-climb-<date>` with before, after, method, run count and targeted tests; `git log --oneline <base>..HEAD` lists them.
4. `LEDGER.md` header reads `DONE`, and the ledger ends with `HILL CLIMB REPORT`: a table of kept wins (target, before, after, method), reverted experiments one line each, unexplained findings, proposals that need <person>, and what to look for in the live system after install.
5. `git status --porcelain` is empty and no process you started is alive.
6. The final message quotes the `HILL CLIMB REPORT` table verbatim and names the branch tip commit.

Also done when three consecutive experiments on different targets yield no kept win, or a decision only <person> can make blocks the rest: write the proposal in the ledger, stop, and name the blocker.

## Output register

<Person> reads the ledger and your final message. Plain words, numbers with units and conditions, no internal ids or coined labels, no time estimates. Short. A table beats a paragraph when comparing.
