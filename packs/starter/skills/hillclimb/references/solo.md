# Solo loop with `hc`

One target, one command-shaped workload, run in this session. The steps assume the frame ([SKILL.md](../SKILL.md), Step 1) is agreed and the metric chosen (Step 2). A campaign lane with a command-shaped workload runs the same loop inside its own worktree.

Set `HC=<this skill's directory>/scripts/hc` (Python 3, stdlib only). `$HC --help` is its full usage.

## 1. Harness

Write the **metric command**. It runs the workload once and exits non-zero on failure. Either `hc` measures its wall time (metric `wall_s`), or it prints a JSON object on its last stdout line with the metric in it. `hc` records every other number in that JSON (a test count, peak memory, the conditions) and shows them beside every comparison, so print the conditions there too: `"load1"` and `"compiles"` (see [measurement.md](measurement.md), A busy or shared machine). Put the command in a script file.

Commit the metric script and every other harness file (fixtures, benchmark config) first. Then create the worktrees, so each one carries the same harness:

- a **candidate** on a new branch: `git worktree add -b hillclimb/<name> <dir>`
- a **kept** worktree at the same commit: `git worktree add --detach <dir> HEAD`

Put both outside anyone else's checkout, and give each its own dependencies: venv, `node_modules`, build output. Check that each one imports code from its own tree, not from the other one. When the workload runs compiled output, build both sides before every A/B with the same flags, and leave the build out of the timed command unless build time is the target. Write the starting commit in the journal; Step 5 measures against it.

From the candidate worktree's root, initialize and lock:

```bash
$HC init <name> --cmd 'bash bench/run.sh' --metric wall_s --direction lower \
  --min-effect 3% --guard '<guardrail check>' --holdout '<held-out command>' \
  --harness bench/ --reps 10
$HC approve
```

`init` creates `.hillclimb/<name>/` with `state.json` and a `journal.md` skeleton. Measurements go in its `runs/` folder, which is gitignored. Run every later `hc` command from that root, or pass `--flow .hillclimb/<name>`.

`approve` locks the harness: the three commands, every `--harness` path (a directory counts as all the files under it), and `hc` itself. `hc run` and `hc ab` then refuse to measure any directory whose copies differ. You approve once at setup and say so in your reply. After that, only the person who owns the climb re-approves, because numbers from a changed harness can't be compared with earlier ones.

Then prove the metric responds. In the candidate worktree, plant a known cost in the code being measured, never in a harness file. A `sleep` or a doubled loop works. Run `$HC ab plant-k plant-e --a <kept> --b <candidate> --reps 5`, then `$HC compare plant-k plant-e`. It must read WORSE at about the size you planted. Remove the plant afterward.

Done when the journal's Setup section is filled in, the planted regression reads WORSE, and `$HC status` shows the harness locked.

## 2. Baseline and noise floor

Measure the untouched code against itself: `$HC ab base base-r2 --a <kept> --b <kept>`, then `$HC compare base base-r2`. The verdict must be WITHIN NOISE, and its CI half-width is the noise floor at this rep count. If the floor is bigger than the minimum effect, no round can prove anything. Fix the measurement first; [measurement.md](measurement.md) lists the fixes in order of cost.

Done when the journal records the baseline median, the noise floor and the conditions, and the floor is below the minimum effect.

## 3. Diagnose

Find where the cost actually goes, with the profiler, trace or waterfall that fits the domain ([domains.md](domains.md)). Rank candidate changes by expected gain, capped by the share of the target each one attacks, and list them in the journal.

Done when each candidate names the measured cost it attacks, with a number from a profile or trace.

## 4. Rounds

One **hypothesis** per round. A change may touch several files, but only in service of one idea about why the number is what it is.

1. Make the change in the candidate worktree. Label the round `eN-<what>`.
2. Measure it against the kept state in the same session: `$HC ab kept-eN eN-<what> --a <kept> --b <candidate>`, then `$HC compare kept-eN eN-<what>`. Re-measure the kept side every round, because earlier numbers have drifted.
3. **Keep** only on BETTER (exit 0), which is the keep bar of [SKILL.md](../SKILL.md), Step 4, in `hc` terms: the interval clears zero on the better side, the effect meets the minimum, the candidate's guard passed, and no rep failed. Also read the "also" lines: a change that helps the target can hurt another number, quietly run fewer tests, or have run under different load.
4. If kept: commit the change alone, with the before and after medians and the interval in the message. Then run `git -C <kept> checkout --detach <commit>`.
5. If not kept: `git -C <candidate> checkout -- <the files you changed>`.
6. Either way, write the journal entry: the hypothesis, the change, the numbers, the verdict, and what it taught you.

After two or three rounds in a row inside the noise, stop changing code and read [stalls.md](stalls.md). Before keeping a win much larger than the change should plausibly give, work through [integrity.md](integrity.md), "When a win looks too good".

Done when the stopping rule fires, or every ranked candidate is tried or written under "Considered, not pursued" with its reason.

## 5. Confirm

- Run the held-out check against the starting commit. Add a worktree there (`git worktree add --detach <dir> <start commit>`), then `$HC ab base-h final-h --holdout --a <that worktree> --b <kept>` and compare. If the gain doesn't carry over, the climb tuned to its benchmark; report that as the result.
- Fill in the journal's Result section: the target at start and end with the interval, each kept change with its gain, the held-out result, what was dropped and why, and the levers still untried.
- Commit `.hillclimb/<name>/journal.md` and `state.json` on the branch (in their own commit, apart from the source wins), then remove the worktrees you created.

Done when the held-out result is in the journal. The final reply leads with the held-out change and names what wasn't verified. Then [SKILL.md](../SKILL.md), Step 5.

## Long runs

Start a measurement that takes minutes as a background job, and give it `--flow` with an absolute path, since a background job runs in the session's working directory. `hc` prints one progress line per rep to stderr (`hc: e3-cache rep 4 exit 0 61.2s`). Check those lines, and the full output in `runs/<label>.log`, before deciding a job is stuck.
