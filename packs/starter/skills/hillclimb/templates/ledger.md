# <Project> performance hill climb: ledger

<!-- The person reads this file. Plain words, numbers with units and conditions, no
internal ids, lane names or coined labels. Keep it to about two screens: one line per
experiment here, the detail in experiments/NNN-<slug>.md. Update it after every
experiment, before starting the next. Delete this comment. -->

**Status:** <RUNNING: phase 1 baseline | RUNNING: experiment NNN on <target> | DONE on <date>, tip <commit>, ready for review and landing | STOPPED: blocked on <decision>>
Updated <UTC time>. Branch `perf/hill-climb-<date>` from `<base>`. Raw data: `<project>-perf-data/<date>/`.

## The hill

Ranked by cost × frequency from [the baseline](baseline.md). Quantities with different units are never added.

| Rank | Target | Cost | Frequency | Status |
| ---: | --- | ---: | --- | --- |
| 1 | <target> | <e.g. 33.6 CPU s/min> | <e.g. continuous, 3 processes> | <not started / experiment NNN kept / reverted / deferred: why> |

## Experiments

One line each, newest last: number, target, the change in a phrase, the result in numbers, and the decision.

- 001 <target>: <change>. <before → after, method, runs, load range>. **Kept** / **Reverted**: <why>. [write-up](experiments/001-<slug>.md)

## Kept wins

| Target | Before | After | Method | Runs | Commit |
| --- | ---: | ---: | --- | --- | --- |
| <target> | <number, unit> | <number, unit> | <metric, paired how, conditions> | <N pairs + independent repeat> | `<hash>` |

## Unexplained

Measurements that moved and that nobody can account for yet. Each one: what was seen, where, under what conditions.

## Needs you

Decisions only <person> can make: a new dependency, a format or contract change, an auth change, a deleted test, a design change. Each one: the proposal, the evidence for it, and what it would unlock.

## Next

The next target and why it is next.

<!-- At the end of the campaign, add this section and set Status to DONE. -->

## HILL CLIMB REPORT

| Target | Before | After | Method |
| --- | ---: | ---: | --- |
| <target> | <number, unit> | <number, unit> | <how it was measured, run count, independent repeat> |

- **Reverted:** <experiment, one line each, with why>.
- **Unexplained:** <one line each>.
- **Needs <person>:** <proposals, one line each>.
- **Owner-accepted on fewer runs:** <which win, which runs completed, which did not>, or none.
- **After install, look for:** <what to compare in the live system, and where to read it>.
- **Not run here:** the full gate, landing, install. They belong to the reviewer and the lander.
