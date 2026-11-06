# <Target>: experiment NNN

<!-- One per change, kept or reverted, at perf/<date>-hill-climb/experiments/NNN-<slug>.md.
Prose and summary tables only; raw rows, profiles and builds live in
<project>-perf-data/<date>/experiments/NNN-<slug>/. Delete this comment. -->

<Date>. Base `<commit>`. Lane branch `<branch>`. Decision: **<Kept in `<hash>` | Reverted | Deferred>**.

## Hypothesis

What costs what, with the profile or trace line that shows it, and why this change should make it cheaper. One idea.

## Change

The exact diff in a sentence or two, and the files. What was deliberately left out.

## Method

- Scenario: the workload, its scale, cold or warm, and why it matches what people do.
- A and B: how each was built (same base, same flags), their hashes, where they are stored.
- Metric and instrument, and how the instrument was validated (a planted cost, a known workload).
- Pairing: order (AB, BA, ...), number of pairs, what made a pair invalid (a compile starting mid-pair, a load mismatch).
- Parity: how A and B were checked to do the same work and give the same results.
- Rerun: the exact command that reproduces the A/B.

## Results

| Pair | Order | A | B | Load (1 min) | Compiles seen | Valid |
| ---: | --- | ---: | ---: | --- | --- | --- |

Median change and its range or interval. The independent repeat's numbers, and who ran it. Secondary numbers that moved.

## Checks

The targeted tests with counts, each new test's red-proof, lint and format. Review: who reviewed it, from which model family, and their findings with dispositions.

## Decision

Kept or reverted, and the reason in terms of the keep bar. What it taught, and what it changes about the hill.
