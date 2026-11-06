# Measurement

How to get numbers the verdict can trust. Read it while building the harness, and again whenever a verdict surprises you.

## What `hc compare` computes

- **Paired mode** (labels measured together by `hc ab`) compares each A rep with the B rep that ran beside it. `hc ab` alternates order A B, B A, A B, and so on, which cancels drift that builds up over a run (thermal throttling, a growing cache, a neighbour's job). The per-pair change is the log ratio `ln(B/A)` when every value is positive, otherwise the plain difference `B − A`. The median of those, bootstrapped 10,000 times with a fixed seed, gives the change and a 95% interval.
- **Unpaired mode** (labels from separate `hc run` calls) compares the two medians and bootstraps each side independently. It can't tell your change from drift between the two sessions, so it warns when the runs are more than an hour apart or on different hosts. Use it for quick looks, and use `hc ab` for any keep decision.
- **Verdict:** BETTER when the interval lies wholly on the better side and the effect meets the minimum effect. BETTER, BELOW MIN EFFECT when it's real but too small. WORSE when the interval lies wholly on the worse side. WITHIN NOISE otherwise. GUARD FAILED (the candidate's guard) or FAILED REPS (the candidate's) override all of these; failed reps on the kept side show as a note. `hc compare` exits 0 on BETTER and 1 on every other verdict. `hc run` and `hc ab` exit 3 when the harness lock is missing or changed.
- **Noise floor:** the CI half-width of an unchanged-vs-unchanged comparison (`base` vs `base-r2`). It shrinks roughly with the square root of the rep count: four times the reps, half the floor.

## Reps

- Start at 10 reps for timings under a minute, 5 for longer ones, and 20 or more when the floor is close to the minimum effect.
- **Price it.** One `hc ab` round runs the workload 2 × (reps + warmup) times, plus the guard once. At 5 reps, a 4-minute suite costs about 50 minutes per round. For a slow workload, loop on a narrower one (the slowest files, one representative job) that takes seconds, and keep the full workload for the held-out check. That usually improves the noise too.
- Add reps to a label with `hc run <label> --append` or `hc ab ... --append`. Both sides need the same treatment, so append to A and B together through `hc ab`.
- Below 5 reps a side, the bootstrap interval is too optimistic; `hc` notes it.
- A warmup rep runs first and is discarded (`--warmup`, default 1). Raise it when the first several runs are slow: JIT compilation, disk cache, lazy compilation, connection pools.

## Quieting the machine

Noise you remove is cheaper than noise you average away.

- **Other load.** `hc` warns when the load average exceeds half the CPU count. When other work shares the CPUs and you cannot stop it, read the next section.
- **Laptop.** Low Power Mode and thermal throttling slow later reps. Interleaving cancels that drift but still widens the noise.
- **Caches between reps** (`__pycache__`, `.pytest_cache`, build and bundler caches, OS file cache): decide whether the metric is cold or warm, and make every rep the same.
- **Cold or warm, decide which.** A cold-start metric has to clear the relevant cache every rep (restart the service, `purge` on macOS, a fresh temp dir). A warm metric needs warmups. Mixing the two produces bimodal data: a few reps far from the rest. Look at the min and max `hc run` prints.
- **Pin what you can.** Fixed seeds, fixed input files, fixed thread or worker counts. Offline where possible: network variance swamps most code changes.
- **Shared resources.** Both worktrees must not share a database, port, or temp directory that one side's run leaves in a different state for the other. Give each its own, or reset it in the metric command.
- **Flaky workloads.** A flaky test fails a rep, and failed reps block a keep. Fix or quarantine the flake with the owner's agreement before the baseline, and record which tests were quarantined.
- **Measure the narrowest thing that still means something.** A single slow test file beats the whole suite for the fast inner loop. Confirm on the full suite at the end.

## A busy or shared machine

On a shared machine, other agents' builds and tests share the CPUs, and the load can swing from 2 to 40 within an hour. That other work is not yours to stop. Generate no artificial load, and wait for no quiet spell that may never come. Measure under the load there is, record it with every sample, and pick a metric the load cannot move much.

**Record the conditions with every sample:** UTC time, 1-minute load, CPU count, and whether a compile was running. Put them in the metric command's last-line JSON, beside the metric, so `hc` keeps them per rep (`hc` stamps the time and CPU count itself):

```bash
load1=$(cut -d' ' -f1 /proc/loadavg)     # macOS: sysctl -n vm.loadavg | awk '{print $2}'
compiles=$(pgrep -c -x 'rustc|cc1|cc1plus|tsc' || true)
printf '{"rss_mb": %s, "load1": %s, "compiles": %s}\n' "$rss_mb" "$load1" "${compiles:-0}"
```

A number without its conditions is never read, and a pair whose two sides ran under very different load (one side over about 1.5× the other's, or a compile that started mid-pair) is discarded as invalid, not averaged in.

**Pick the metric by how much the load can move it,** least first:

1. **Deterministic counts:** bytes, allocations, queries per request, RSS at a fixed point, user-space instructions retired. On Linux: `perf stat -x, -e instructions:u -- <cmd>` around a command, or `perf stat -x, -e instructions:u -p <pid> --per-thread` on a running process, delimited with `--control` or `-D` so it counts only the active window. Instruction counts hold steady under compile load, when wall time is useless. macOS has no `perf`; use a deterministic count or Instruments' counters there.
2. **Process CPU time** (user + system seconds of the processes doing the work, from `/proc/<pid>/stat` or `getrusage`) under recorded load. Contention inflates it less than wall time, but it still moves, so it needs pairs.
3. **Wall time** only in paired runs with matched conditions, and only when wall time is what people wait for. Confirm a count-based win with a timing once the machine allows, because a count is a proxy.

**Validate the counter before trusting it.** For `perf stat`, a `<not counted>` or `<not supported>` value is invalid, never zero; time enabled must equal time running (otherwise the counter was multiplexed and scaled); and on a multi-threaded process every thread that does the work must be counted, with the thread set unchanged across the window. A count that misses sleeping threads reads low, so reject it. Plant a known cost and watch the counter read it before the first A/B.

**Memory:** measure the process's live RSS (`VmRSS` in `/proc/<pid>/status`, or PSS from `smaps_rollup`), not bytes the program thinks are reachable. Measure cold and warm separately, and at the real scale (the real number of sessions, files or rows), because a bounded cache only shows its bound at scale.

## Choosing the metric command

- It runs the workload **once** and exits 0 on success. `hc` does the repetition, timing, and statistics.
- For a metric other than wall time, print a JSON object on the last stdout line: `{"p95_ms": 41.2, "rss_mb": 312}`. `hc` records every numeric key, and the one named by `--metric` drives the verdict. The rest show up in the rows for checking side effects.
- Prefer a metric the workload reports itself (a benchmark's own ns/op, a server's latency histogram) over wall time around a process that also pays startup cost, unless startup is part of what users wait for.
- Deterministic counts beat timings when they measure the same thing. Bytes shipped, allocations, queries per request, and instructions retired (`perf stat -e instructions` on Linux) have almost no noise, so one rep suffices. Confirm with a timing at the end, because a count is a proxy.
- Percentiles need enough samples inside each rep. A p99 from 200 requests is dominated by one or two of them.

## When a verdict surprises you

- **Bimodal reps** (look at min vs max): something alternates between two states, like cache hit vs miss or a GC firing or not. Find it and pin it before continuing.
- **The interval won't shrink with more reps:** drift within a session that interleaving doesn't cancel, or a metric that changes over time (a database that grows each run). Reset state inside the metric command.
- **The unchanged-vs-unchanged comparison doesn't read WITHIN NOISE:** order effects (the first directory measured is always slower), or the two directories aren't actually identical. Check `hc status` fingerprints: git head and dirty diff.
- **Other numbers moved.** `hc compare` prints an "also" line for every other number the metric command reported. `runs/<label>.jsonl` has every rep.
