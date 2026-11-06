---
name: hillclimb
description: Hill-climb: make a measurable number better (speed, latency, memory, size, build or test time, cost), keeping only wins the measurements prove. Solo rounds with the hc harness, or a helper campaign.
---

# Hill-climb

Make one number better, or a whole product's hot paths in a campaign, through measured **experiments**. Each experiment makes one change and measures it against the last **kept** state. The change stays only when the gain clears the **noise floor** and the **minimum effect**, and every **guardrail** holds. Every experiment, kept or reverted, gets written down.

Terms used throughout:

- **Target:** the number being improved, with its direction and the workload it is measured on.
- **Noise floor:** how far the number moves when nothing changed (the 95% interval half-width of unchanged against unchanged).
- **Paired runs:** A and B measured back to back, alternating order (A B, B A, ...), so drift hits both sides alike.
- **Conditions:** what the machine was doing during a sample: UTC time, 1-minute load, CPU count, compiles running. Each sample carries them.
- **Hill:** candidate targets ranked by cost × frequency (a 40 ms cost paid 3,000 times a day outranks a 2 s cost paid twice).
- **Ledger:** the campaign's running record. It is what the person reads.

## Rules for every climb

- Baseline first. No code change until the baseline exists and the hill is ranked.
- Measure under natural load and record the conditions. Generate no artificial load to see how something behaves under load, and wait for no quiet machine that may never come.
- Keep only measured wins. A reverted experiment leaves its write-up and no code.
- Plain words in the ledger and the final message: numbers with units and conditions, no internal ids, coined labels or time estimates.
- The live system is read-only. Read its logs and `/proc`. Write nothing to its state or config, and stop only processes you started, by their PID or the unit you started them in.
- The climbing session runs no full gate, lands nothing, and pushes to no protected branch. Review and landing come after it (Step 6).
- A climb graded from model output (prompts, skills, model or effort choice) follows [domains.md](./references/domains.md), "Model output", before its first scored run.

## Bundle map

Paths are relative to this skill's directory. Read a file when a step sends you there.

| Path | Holds |
| --- | --- |
| [solo.md](./references/solo.md) | The solo loop with `hc`: harness, noise floor, diagnosis, rounds, held-out check |
| [campaign.md](./references/campaign.md) | The campaign: worktree and data layout, baseline phase, the hill, experiment lanes, review, ledger, stopping, hand-off |
| [orchestrator.md](./references/orchestrator.md) | Starting the campaign's orchestrator helper, checking on it, steering, taking the hand-off |
| [measurement.md](./references/measurement.md) | Noise, reps, interleaving, a metric that survives a busy machine, choosing the metric command |
| [integrity.md](./references/integrity.md) | Guardrails, the held-out check, the harness lock, ways a benchmark gets gamed, wins that look too good |
| [domains.md](./references/domains.md) | Per-domain recipes: metric, guardrail, held-out check, where the cost hides, first levers |
| [stalls.md](./references/stalls.md) | What to do when experiments stop moving the number |
| [templates](./templates/) | `brief.md`, `goal.md`, `ledger.md`, `experiment.md`, `lanes.md`: the campaign's documents, with blanks to fill |
| [hc](./scripts/hc) | The measuring harness: paired runs, the noise verdict, the harness lock. `hc --help` is its usage |

## Steps

### 1. Frame

Agree on these with the person who asked. Propose a default for each so they can just say yes:

- **Target:** the metric, its direction, and the workload, stated as what a user or operator waits for or pays for. In a campaign the target is a theme ("the things agents do all the time"); the baseline turns it into a ranked hill.
- **Minimum effect:** the smallest gain worth keeping a change for, such as 3%.
- **Guardrails:** what must not get worse, each with the check that proves it. Default: the targeted tests of the touched code pass and behavior is unchanged.
- **Held-out check:** a workload you never tune on, run at the end ([integrity.md](./references/integrity.md)).
- **Scope:** what may change and what is off-limits. Name what needs the owner: new dependencies, on-disk or wire format changes, auth changes, deleted tests, design changes.
- **Stopping rule and budget:** a round count, "three experiments in a row with no kept win", or a time cap. Price one experiment before agreeing ([measurement.md](./references/measurement.md), Reps).

Read [domains.md](./references/domains.md) for the matching recipe.

Done when every item has an agreed value written down.

### 2. Pick a metric that survives the machine

Check the machine before choosing: `uptime` against `nproc`, and whether builds run beside you (`pgrep -c -x 'rustc|cc1|cc1plus|tsc'`, or the project's own compiler names). On a shared box where load swings, a wall-time A/B mostly measures the neighbours. Pick the metric from the ladder in [measurement.md](./references/measurement.md), "A busy or shared machine": deterministic counts first (instructions, bytes, allocations, RSS), process CPU time under recorded load next, and wall time only in paired runs with matched conditions.

Done when the metric, its instrument, and the conditions recorded with every sample are written down (in a campaign, in the brief). Before the first A/B, the instrument must also have read a planted cost at about its real size.

### 3. Choose the shape

| Solo ([solo.md](./references/solo.md)) | Campaign ([campaign.md](./references/campaign.md)) |
| --- | --- |
| One target, one workload that runs as a command | A theme across a product; the targets are unknown until the baseline |
| You, in this session, for hours | An orchestrator agent for most of a day, with helper lanes |
| One subsystem; changes are yours to make | Several subsystems; parallel measurement and change lanes |
| The journal lives in `.hillclimb/<name>/` | A ledger the person reads while it runs |

A campaign lane that has a command-shaped workload runs the solo loop with `hc` inside its own worktree. Both references apply Step 4 to every experiment; after the reference's last step, continue at Step 5.

### 4. Keep only what the numbers prove

A win is kept when, with the guardrails green:

- paired runs show it: at least five pairs with a consistent sign, or non-overlapping ranges, or `hc compare` reads BETTER;
- the samples' conditions are comparable (a before at load 5 against an after at load 30 is not a measurement);
- the mechanism is explained by a profile or trace line that got cheaper, of roughly the right size;
- it repeats: in a campaign, a run by someone other than the lane that wrote the change.

Before keeping a win much larger than the change should give, work through [integrity.md](./references/integrity.md), "When a win looks too good". Something that moved and that you cannot explain is a finding: write it under Unexplained and move on.

Done when each experiment has its decision, kept or reverted, with the numbers and conditions that decided it.

### 5. Done state

- Each kept win is its own commit with explicit pathspecs, source only: the subject is at most 72 characters, and the body gives before, after, method and run count, plus the targeted tests with their counts and the red-proofs.
- The record is complete: the solo journal's Result section, or the ledger's header saying done and its final `HILL CLIMB REPORT`.
- `git status --porcelain` is empty, raw data sits outside the repository, and no process you started is still alive.
- The final message quotes the kept-wins table verbatim and names the branch tip commit.

### 6. Review, then land

Hand the source commits to an independent reviewer from the other model family than the one that wrote them: a helper with `kind: reviewer` and a model from that family, or the `cross-family-review` skill. Its findings go to a fix lane on the same branch. Landing goes through the project's own path, which the coordinator or the person runs, with the full gate; the climbing session hands off and stops. After install, the ledger's "what to look for live" section says how to confirm the win on the real workload.

## Resuming

Read the journal or ledger first, then `git log <base>..HEAD` on the branch, then `hc status` for a solo climb, or the orchestrator helper's state and the ledger header for a campaign. Before touching anything, re-arm whatever watched it: the background measurement job, or the wait on the orchestrator.
