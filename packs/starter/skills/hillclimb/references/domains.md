# Domains

Starting recipes. Each gives a metric command shape, the guardrail, a held-out check, where to look for the cost, and the first levers. Adapt freely. The method in [SKILL.md](../SKILL.md) stays the same.

In every domain, each worktree gets its own dependencies (virtualenv, `node_modules`, build output); an editable install (`pip install -e`) points at its own tree; and both sides run the same worker or thread count.

## Test-suite time

- **Metric:** `wall_s` around the suite command (`pnpm test`, `pytest -q`, `go test ./...`, `cargo test`). A failing test fails the rep, so the suite is its own correctness check.
- **Guardrail:** the same tests run and pass. Save the collected test list once (`pytest --collect-only -q | grep '::' > bench/tests.txt`, which drops the summary line and its timing; or the runner's own list mode) and make `--guard` diff a fresh collection against it; put `bench/` on the harness list. Also print the passed-test count as JSON on the metric command's last line, so a drop shows in every comparison.
- **Held-out:** tune on the slowest files, confirm on the whole suite. Confirm in CI too, where parallelism and caches differ.
- **Where the time goes:** per-test durations (`pytest --durations=30`, `go test -json` elapsed fields, the JSON reporters of vitest and jest), plus setup and teardown, collection or import time (`python -X importtime`), and per-worker imbalance: one worker finishing last sets the wall time.
- **Levers:** shared fixtures instead of per-test setup; in-memory or transaction-rollback databases; removing real sleeps and timeouts (fake timers); worker count and sharding balance; lazy imports; dropping duplicated coverage only with the owner's agreement, since that is removing tests.

## Build and typecheck time

- **Metric:** `wall_s` of a clean build and, separately, an incremental build after touching one representative file. They respond to different levers; climb the one people actually wait on.
- **Guardrail:** output unchanged. Hash the build directory, or diff the emitted files after normalizing timestamps.
- **Where the time goes:** `cargo build --timings`, `tsc --extendedDiagnostics` or `--generateTrace`, bundler profiles (`vite build --profile`, webpack's `--profile --json`), `go build -x`.
- **Levers:** cache configuration and hit rate, project references or workspace splitting, excluding generated or vendored code from typechecking, faster transpilers for dev builds, dependency trimming, parallelism.

## CLI, library, or batch-job runtime

- **Metric:** a fixed-input invocation, wall time or the program's own timing. Use a deterministic count (instructions, allocations) for the inner loop when noise is high.
- **Guardrail:** byte-identical output on a set of fixed inputs, hashed in `--guard`.
- **Held-out:** a different input size or shape: the 10× file, the pathological case, a real production sample.
- **Where the time goes:** a sampling profiler. `py-spy record`, `samply record`, `perf record` (Linux), Instruments (macOS), `go tool pprof`, `node --cpu-prof`.
- **Levers:** algorithmic first (the quadratic loop, repeated work, needless I/O), then data layout, then allocation and copying, then I/O batching and buffering, then parallelism, and micro-optimizations last.
- **Layout before a cache:** when the profile shows pointer chasing, scattered allocations or repeated hash lookups on a hot path, try a different layout before adding a cache: flat arrays with index references instead of pointers, a struct of arrays when a loop reads one field across many records, cold fields split away from hot ones. A cache adds invalidation and memory; a layout change can remove the cost the cache would hide. The trade-offs are index capacity limits, harder debugging, and other access patterns getting slower, so run the held-out check on a different access pattern, not only a different input size.

## Service latency and throughput

- **Metric:** a load generator at a fixed request rate (for latency) or fixed concurrency (for throughput), printing percentiles as JSON. Hold the load constant across rounds. Latency measured at saturation is mostly queueing.
- **Guardrail:** response bodies and status codes unchanged on a fixed request set; error rate zero.
- **Held-out:** replay of real traffic, or a different mix of endpoints.
- **Where the time goes:** request traces and spans, slow-query logs, profiler under load, connection pool wait times.
- **Levers:** query count per request (N+1), indexes, caching with an explicit invalidation story (for an in-process hot path, try the runtime recipe's layout lever first), payload size, connection reuse, moving work off the request path (a trade the owner signs off on; see [integrity.md](integrity.md)).

## A long-running system's footprint

Daemons, servers, agent harnesses, anything that stays up while people work: the campaign shape ([campaign.md](campaign.md)) usually fits.

- **Metric:** per process kind, CPU seconds per minute and RSS, sampled from `/proc` (or `ps -eo pid,ppid,rss,pcpu,etimes,args`) at a steady tick; per-operation latency from the timing the system already logs; counts of short-lived processes it spawns per minute. Instruction counts or process CPU for the A/B when the machine is busy ([measurement.md](measurement.md)).
- **Two workloads:** the live system is the baseline and the final confirmation, read-only. A/B runs on a private copy (its own state directory, a fake or stub for paid upstream calls, the real scale of sessions, files or rows) that you start and stop yourself.
- **Guardrail:** the private copy's results match between A and B (same responses, same ordering, same subscriptions or events delivered), plus the targeted tests.
- **Held-out:** the live system after install, compared on the same quantity under similar conditions. The ledger says what to look for.
- **Where it goes:** idle cost (redraw or poll loops that run with nothing to do), caches and handles that grow with history (readers, page caches, retained sessions), spawn storms (helpers and probes per operation), startup work repeated per session.
- **Levers:** bound what grows with history, release what belongs to ended work, borrow instead of clone on hot redraws, batch or cache spawned probes, make idle loops wait on events.

## Memory

- **Metric:** peak RSS from the metric command's own report, or from `/usr/bin/time -l` (macOS, bytes) or `/usr/bin/time -v` (Linux, KB), converted in the JSON to one unit. Or heap-in-use from the runtime's own stats.
- **Guardrail:** wall time must not regress past its noise. Memory wins often cost speed; put wall time in the JSON.
- **Where it goes:** heap profilers (`memray`, `heaptrack`, pprof heap, Chrome heap snapshots, Instruments Allocations).

## Size: bundle, binary, image

- **Metric:** bytes, compressed as served (brotli or gzip for web assets). Deterministic: one rep is enough and the noise floor is zero, so the minimum effect does all the filtering.
- **Guardrail:** behavior unchanged (the test suite, plus screenshots for UI).
- **Where it goes:** `source-map-explorer` or a bundle visualizer, `cargo bloat`, `go tool nm -size`, `docker history` or `dive` for images.
- **Levers:** dead dependencies, duplicate versions of one package, per-route code splitting, stripping symbols, smaller base images, multi-stage builds.

## Cost

Cost is usually a derived metric: requests × tokens × price, or instance-hours × rate. Measure the countable parts deterministically (tokens per task, queries per request, CPU-seconds per job), and hold quality or latency as the guardrail.

## Web page load

- **Metric:** a lab metric from a throttled, fixed profile (largest contentful paint, total blocking time, transferred bytes), the median of at least five loads per side. Bytes are deterministic; timings need pairs.
- **Guardrail:** the rendered page unchanged (pixel diff of screenshots at fixed viewports), plus the project's tests.
- **Held-out:** the other page templates, and real-user field data once deployed.
- **Where the time goes:** the request waterfall, the main-thread trace, the largest elements and the scripts that block rendering.
- **Levers:** fewer and smaller bytes before the critical render, images sized and lazy-loaded, deferred third-party scripts, caching headers, fewer round trips.

## Model output

Prompts, skills, model or effort choice, anything graded from a model's output. Build the eval first when there is none: fixed inputs, a grader, and a held-out set of inputs the climb never tunes on.

- **Metric:** the eval's score, with the run's cost and latency beside it in the JSON. Model output varies run to run, so use many items and reps; the noise floor decides what a prompt change can prove.
- **Plant four answers** in the real runner and grader before the first scored run: a known-good one, an empty one, a confidently irrelevant one and a timed-out one. Each must land on its own recorded outcome. A grader that scores a crash the same as "found nothing" makes a broken runner read as a clean result.
- **Guardrail:** the cases that passed before still pass, and cost and latency hold.
- **Held-out:** inputs the prompt was never tuned on; a gain that vanishes there is overfitting to the eval.
- **Levers:** a shorter prompt, a clearer instruction, a cheaper model or lower effort where the score holds, cached prefixes, fewer calls per task.
