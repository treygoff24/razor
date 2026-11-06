# Orchestrator: start it, check on it, take the hand-off

The campaign's **orchestrator** is one long-lived agent pursuing the brief across many turns. It is either you, the session the person talks to, or a helper you start. A helper keeps your own context free and lets the person keep working with you while the campaign runs. The **starter** is whoever started it: the person, or the agent they asked. The starter checks on it, steers on drift, answers its questions and takes the hand-off. When you orchestrate yourself, you hold both roles: run the cycle in Check on it against your own state.

## Start

Prepare the campaign first ([campaign.md](campaign.md), Set up): the brief, goal and ledger skeleton are committed on the campaign branch. Then start the helper in the campaign worktree, working in place so it commits on the campaign branch itself:

- `agent` with `action: start`, `name: hillclimb`, `cwd:` the campaign worktree, `placement: in_place`, `model` and `effort` of your choosing (`high` or above for an orchestrator; it makes ranking and keep decisions), and `brief:` the kickoff from [goal.md](../templates/goal.md), filled in.

The helper sees nothing of your work, so the kickoff names the brief and its commit, the order of work, the done conditions as things it will print, and the boundaries. Add that the helper starts its own helpers for lanes, and that anything only the person can decide goes in the ledger's "Needs you" section while it carries on.

Done when the helper's first report or the ledger shows the baseline under way.

## Check on it

`agent` with `action: wait` returns at the helper's next report, mail or question, and covers every other running helper and background job in the same call. Use it as the wake. On each wake, and when a wait times out with no word, run:

```bash
sed -n 1,40p <campaign worktree>/perf/<date>-hill-climb/LEDGER.md
git -C <campaign worktree> log --oneline <base>..HEAD
git -C <campaign worktree> diff --shortstat <base>..HEAD   # millions of lines = raw data committed
git -C <campaign worktree> worktree list
uptime
```

Steer only on **drift**. A healthy campaign looks slow from outside: a lane sampling for twenty minutes with a quiet ledger is working. Drift is one of these:

| Sign | Steer |
| --- | --- |
| A code change before `baseline.md` ranks the hill | Back to the baseline; no change until the hill exists |
| A number in the ledger without its conditions or run count | Record load, compiles and run count, or drop the claim |
| Wall-time A/B while load swings | Switch to the busy-machine metric the brief names |
| Raw data in commits (`--shortstat` jumps, `baseline/` or `experiments/` holding JSON rows) | Move it to the data folder, add the `.gitignore` entry, rewrite the docs commits before hand-off |
| More lanes compiling than the cap | Hold new compile lanes until one finishes |
| Anything written under the live system's state, or a process stopped that it did not start | Stop it at once; this is the hardest boundary |
| The next experiment picked by ease rather than cost × frequency | Re-rank and take the top |
| Several changes bundled into one experiment | Split them |
| The ledger more than one experiment behind | Update the ledger before the next lane |
| The helper stopped without the ledger reading DONE | Read its last report, then continue it onward |

**Steering** is `agent` with `action: continue`, `child: hillclimb`, and a short `brief` that makes one point, in the person's voice only when they asked you to steer for them. Steers that worked: "hand off after the memory target; the remaining targets are a follow-up campaign", and "you have enough; stop the remaining pairs, record what completed and what did not, commit, declare done, reply with the final hash". A steer that sets direction names the standard: "Measure live RSS, not reachable bytes; cold and warm separately, at full scale, paired runs, an independent repeat, correctness tests red-proved."

A question the helper cannot settle from the brief reaches you by `message`. Answer from the brief; when only the person can answer, put it to them and tell the helper to record it under "Needs you" and carry on.

Hold anything destructive, tree-wide in git, aimed at the live system, or outside the brief, and bring it to the person.

## Take the hand-off

The helper ends with `agent finish`. Verify the done state yourself rather than trusting the report ([SKILL.md](../SKILL.md), Step 5, and [campaign.md](campaign.md), Hand-off): the ledger header, `git -C <campaign worktree> status --porcelain` empty, the tip, each kept win its own commit with before, after, method and run count, and the kept-wins table in the final message. A helper working in place has no branch of its own: its commits are already on the campaign branch, so settle it with `decision: discard`, which leaves them in place. Then start the review ([SKILL.md](../SKILL.md), Step 6).
