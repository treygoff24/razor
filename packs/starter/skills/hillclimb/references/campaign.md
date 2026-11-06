# Campaign

A campaign climbs a whole product's hot paths in one long run. An **orchestrator** agent owns the campaign branch and the ledger. It starts **lanes** (helpers with one bounded job each) to measure, change, review and scout, and it keeps only what its own repeat of the numbers proves. The person who asked reads the ledger and steers; whoever started the orchestrator keeps it moving ([orchestrator.md](orchestrator.md)).

The campaign runs in two phases, in order: a **baseline** of the live system that ends in a ranked hill, then **experiments** from the top of the hill down, one change each.

## 1. Set up

Run these as the agent preparing the campaign, before the orchestrator starts. `<p>` is the project's short name, `<date>` today's date.

1. **Pick the verified base:** the newest commit that passed the project's full gate (a `green` branch, the last tagged release, the commit CI last passed). Experiments are measured against it, so an unverified base turns every regression into a mystery.
2. **Lay out the worktree and the data folder** as siblings of the project's checkout:

   ```bash
   git -C <project> worktree add -b perf/hill-climb-<date> <project>-perf <base>
   mkdir -p <project>-perf-data/<date>/{baseline,experiments,logs,patches}
   ```

   | Place | Holds | In git |
   | --- | --- | --- |
   | `<project>-perf` | The campaign worktree on `perf/hill-climb-<date>`; read-only measurement lanes share it | The branch |
   | Each change lane's worktree | One per change lane, on its own branch cut from the campaign branch (`placement: worktree`) | Lane branches, merged into the campaign branch only when kept |
   | `<project>-perf-data/<date>/` | Raw samples, per-rep rows, profiles, logs, patches, private A and B builds | Never |
   | `perf/<date>-hill-climb/` in the campaign worktree (the **campaign folder**) | `BRIEF.md`, `GOAL.md`, `LEDGER.md`, `baseline.md`, `experiments/NNN-<slug>.md`, `briefs/`, `scripts/` | Yes |

   **Commit prose and small scripts only.** Raw data stays in the data folder, and the write-ups quote numbers and name the data path. Anything generated, or over about 100 KB, belongs in the data folder. A campaign once committed megabytes of raw runs across hundreds of files before anyone checked, so compare `git diff --shortstat <base>..HEAD` at every check. If a tool insists on writing inside the tree, add its output folder to `.gitignore` in the first commit.

3. **Write the brief** from [brief.md](../templates/brief.md) into the campaign folder as `BRIEF.md`, and the kickoff text from [goal.md](../templates/goal.md) beside it as `GOAL.md`. Fill every blank from the real project: the live system's processes and state paths, the timing data it already records, the instruments already in the repo, the targeted-test commands, the compile cap. Copy the ledger skeleton from [ledger.md](../templates/ledger.md) to `LEDGER.md`.
4. **Commit the brief, goal and ledger skeleton** on the campaign branch, explicit pathspecs. The kickoff names that commit, so the orchestrator works from a fixed contract, and every lane's worktree starts from a commit that carries it.
5. **Start the orchestrator** ([orchestrator.md](orchestrator.md)), or work as the orchestrator yourself.

Done when the campaign branch carries the committed brief, goal and ledger skeleton, the data folder exists outside the repository, and an orchestrator is running the goal.

## 2. Baseline (phase 1)

Read-only. No code changes, no compiles while sampling. One lane per area, all sharing the campaign worktree (`placement: in_place`, `cwd` the worktree) and each owning only its own artifacts ([lanes.md](../templates/lanes.md), Measurement lane):

- **Processes:** every process the system owns, over time: count, kind, lifetime, parent. Bursts of short-lived processes per minute and which paths spawn them.
- **CPU and RAM:** per process kind, CPU seconds per minute and RSS, including the idle cost of each long-lived process.
- **Disk:** state size and growth, the largest logs and databases, writes per second where `/proc/<pid>/io` shows them.
- **Latency:** per operation, the count, median, p95 and total, from the timing data the system already logs. Split the system's own overhead from time spent waiting on a model, the network or a human.
- **Journeys:** the handful of end-to-end actions people repeat, each timed with the repo's rig or from the live logs.

Each lane returns a ranked table (target, cost, frequency, aggregate cost, evidence path and lines, likely code path, what is still unknown), plus raw JSON in the data folder. Each sample carries its conditions. Historical logs have no recorded load, so say so instead of borrowing today's.

The orchestrator then writes `baseline.md`: the ranked hill (top ten, each with cost, frequency and its evidence line) and a conditions table. Quantities with different units stay in separate columns and never get added together (CPU seconds, elapsed seconds, MiB).

Done when `baseline.md` names the top ten targets, each with cost, frequency and evidence, and the ledger shows the hill.

## 3. Experiments (phase 2)

Climb from the top of the hill down. Pick the next target by expected cost × frequency saved, not by ease. Each experiment follows the same loop:

1. **Scout** (optional, read-only): a lane maps the target's code path and returns at most three source-backed candidate changes, each with the smallest scenario that would measure it. A scout can run while the orchestrator measures something else.
2. **Change lane:** one helper with `placement: worktree` (its own worktree and branch from the campaign branch's last commit), one change ([lanes.md](../templates/lanes.md), Change lane). It builds private A and B from the same base with the same flags and keeps both with their hashes in the data folder. An installed copy of the product is different source and is never the A side. It runs paired runs with conditions per sample, checks that A and B did the same work and gave the same results, runs the targeted tests of the touched code, red-proves each new test, and writes `experiments/NNN-<slug>.md` from [experiment.md](../templates/experiment.md). It commits the source change alone, then the write-up in a second commit, and finishes with its report.
3. **Review lane**, a reviewer helper from the other model family, read-only: reachable behavior changes in the diff, and ways the measurement could produce a false positive ([lanes.md](../templates/lanes.md), Review lane). Make it the norm for every experiment, not only the large ones.
4. **Independent repeat:** the orchestrator re-reads the lane's numbers and re-runs the A/B itself, outside the lane, against the lane's branch. A lane inside a sandbox is sometimes refused by the product (a sandboxed process service that will not start, for one). When that happens, the lane hands over its runnable command and the orchestrator's run is the measurement.
5. **Decide** by the keep bar ([SKILL.md](../SKILL.md), Step 4). If kept: settle the lane with `merge`, so the source commit and the write-up land on the campaign branch, then update the ledger in its own docs commit. If reverted: cherry-pick the write-up commit onto the campaign branch first, then settle the lane with `discard`, so the write-up stays and the code goes.
6. **Update the ledger** ([ledger.md](../templates/ledger.md)) before starting the next experiment.

Re-rank the hill after every three kept wins, because a win can move the next bottleneck.

## 4. Lanes

- **Models:** use the aliases `model` accepts: `opus`, `sonnet`, `haiku`, `fable` (Claude family) and `sol`, `terra`, `luna`, `astra` (GPT family). Mix model families across lanes so each subsystem gets more than one family's eyes. Effort: `low` for mechanical sampling scripts, `medium` or `high` for analysis and changes, `xhigh` only after `high` fell short.
- **Compile cap:** at most two lanes build at once on a shared machine, through the project's shared build slot if it has one. A lane never compiles while it is sampling.
- **A lane brief states:** owned files, read-only areas, what it decides alone, what it stops for, its artifact paths (prose in the worktree, raw data in the data folder), and the checks it runs. Skeletons are in [lanes.md](../templates/lanes.md). A lane sees nothing of the orchestrator's work, so the brief carries everything it needs.
- **A lane's report is a claim.** Re-read its numbers before using them.
- **Questions go to the orchestrator, decisions go in the brief.** Give each lane enough to decide alone; a lane that waits on a routine question idles a slot.
- **Waiting:** `agent wait` covers every running lane and job at once. Do other work, such as the ledger or the next scout, until it returns.

## 5. Stopping

Stop when the first of these holds:

- every top-ten target has an experiment write-up, kept or reverted;
- three experiments in a row, on different targets, yield no kept win (what is left is below the noise floor or needs a design change);
- a decision only the owner can make blocks the rest: write the proposal in the ledger's "Needs you" section, stop, and name the blocker;
- the budget is spent, or the person says stop.

**The bar for an owner-accepted win.** The session holds itself to the full keep bar. Only the owner may accept a win on fewer runs than the brief requires, and the session may propose it in the ledger when all of these hold:

- the effect is many times the spread, so the ranges sit far apart (a memory footprint that fell from hundreds of MiB to tens on the first full-scale pair);
- correctness tests that guard the change exist and were red-proved;
- an independent review of the source follows before landing.

Keep the full bar when the effect is within a few noise floors, when the metric is wall time on a loaded machine, when no independent repeat exists, or when the change trades against a guardrail. When the owner accepts, record which runs completed and which did not, in the ledger and the commit body. A run that did not happen is never relabeled as a pass.

## 6. Hand-off

The done state is [SKILL.md](../SKILL.md), Step 5, plus:

- the ledger header reads `DONE` with the date and the branch tip;
- every lane is settled, and the ledger names any lane branch left unmerged;
- the final message quotes the `HILL CLIMB REPORT` kept-wins table verbatim and names the tip.

The reviewer and the landing path follow ([SKILL.md](../SKILL.md), Step 6). Docs commits can land with the source or be left on the branch; that is the lander's call, which is why they stay separate from the source commits.
