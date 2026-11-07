# Enforcement: redirects, brief modules and task encoding

Ready-to-send redirect prompts for live drift, the credit-rules module that travels in every code-writing brief, and how to encode these rules in tasks so they reach agents at claim time.

## Redirect prompts

### Anti-ceremony redirect (a helper, or yourself, drifting into process work)

```text
Stop. The artifact you are building (<NAME>) gates no named feature. Our goal
is working, deployable capability; process exists only to serve that. Park
the artifact, note why it was stopped, and take the highest-priority ready
capability item instead: <ITEM or 'take the top ready task'>. If you believe
the artifact IS a hard gate, reply with one sentence naming its consumer, the
feature it gates, and the observed defect class that justifies it; otherwise
drop it.
```

### Machinery freeze / render now (governance has become the ceremony)

```text
STOP process work. We have fallen into the trap this skill forbids:
<N> rounds of <schema/validator/audit/review> work while the deliverable
stands at <X>/<Y>. Effective immediately:
1. Machinery is FROZEN at current HEAD. The existing checks are sufficient to
   keep authoring honest. Record deferred rigor as an explicit debt list; do
   not build it.
2. Begin producing the actual deliverable NOW, in bounded batches, with
   <verification floor> after each batch.
3. Reviews are welcome on CONTENT (deliverable quality, correctness), not on
   machinery. No new machinery demands until the deliverable is done.
The deliverable is the deliverable. Acknowledge and start.
```

### Refusal-farming redirect

```text
Your recent closes are guard/refusal paths; the positive capability is still
open. A typed refusal beats a fabricated result, but it is far less valuable
than the real capability. Label the current item refusal-only, leave it open,
and implement the positive path now: <SPECIFIC POSITIVE BEHAVIOR + TEST>.
Pair every forbidden case you already wrote with a near-identical permitted
case that proceeds.
```

## Credit-rules module

A helper started with the `agent` tool receives its startup files and brief, but not your conversation. Append this to any brief that writes code, tests, or gates:

```text
CREDIT RULES (binding):
- Code changes need focused verification and meaningful tests for nontrivial
  behavior in the SAME work item. Prose and read-only tasks need
  artifact/source checks, not invented code tests. No placeholder scaffolds
  in commits. No faked tests, no fixtures/mocks presented as live proof, no
  weakened assertions, no golden regeneration to force green, no hard-coded
  success paths, no environment sniffing.
- Tests exercising a feature belong with that feature. Each relaxation of a
  test, validator, gate, CI, or lint rule (skipped test, suppression pragma,
  widened tolerance/timeout) gets separate review and a one-line
  justification; separate its commit from the feature when commits are in
  scope.
- A requested plan, report, or prose artifact is the deliverable. Additional
  process artifacts are not capability progress; create one only if it is a
  hard gate for a named deliverable, and name its consumer, gate, observed
  defect class, and deletion condition.
- Do not close or certify your own work; report it done-pending-verification
  with the exact commands a verifier should re-run. If reporting done with
  no verifier, re-execute the acceptance criteria first and state what was
  not independently verified.
- If blocked on something external, say blocked, naming the exact missing
  thing AND the substitutes you are forbidden to fake it with. Waiting earns
  zero credit; fabricating earns negative credit.
- Never claim a command or test ran unless you ran it and observed the
  output. Never silence stderr in a command cited as evidence. A truthful
  "checked X, found nothing" is a successful result.
- Commit rate is not a KPI. Splitting work to harvest closures is reward
  hacking.
```

## Brief shape: positive observable, planted negative, No-Claim

Brief the behavior to establish rather than "make the tests pass" or "get CI green." Every acceptance criterion in a brief carries:

- **Positive observable**: the behavior through the public surface, with the exact command and expected output, not an exit code alone.
- **Planted negative**: one input a naive wrong implementation would fail, near-identical to the positive, differing only in the forbidden dimension.
- **No-Claim line**: one sentence naming what green does NOT prove ("green here proves the parser accepts the fixture set; it does not prove live API compatibility"). This is the single highest-leverage line: it pre-writes the claim a gamed result would need to make, turning proof-class inflation into a violation of the brief's own text.

## Encoding the rules in tasks

For work that spans sessions or many helpers:

1. **Put the criteria in the task.** Write acceptance as positive observables and planted negatives, and add the No-Claim line to the task body where inflation is plausible. A change to the criteria is a recorded decision, not an edit.
2. **Review gates and decisions that need the person are blocking tasks** (`blocked_by`), so the only reachable close route for downstream work runs through the gate: policy, not etiquette.
3. **Blocked-born external leaves.** Work waiting on an external credential, authority or subject is born blocked, naming (a) the exact missing thing and (b) the forbidden substitutes an agent would be tempted to fake it with. With no live route at ship time, the report says BLOCKED on external proof, never green with a footnote.
4. **Know which invariants your tooling enforces and which you must police.** The second kind silently decays when you stop checking; after any tooling change, re-test that an illegal close is actually refused before trusting it in a real wave.

## Incentive firewall

Tell helpers the exact object-level question, evidence access and boundaries, valid operations and outputs, and that null and blocked outcomes are allowed. Reward supported object-level work rather than findings, novelty, disagreement, confidence, verbosity, or antagonism. When a requested conclusion is unsupported, useful object-level work remains expected.

## Verification-tooling honesty

When building validators or audit tools: partial-input runs report INCOMPLETE with nonzero exit, never PASS; structural defects short-circuit as FAIL before INCOMPLETE; every report carries its own unimplemented-checks list (the honest result is UNAVAILABLE, not PASS-by-omission); agreed baselines refuse silent refresh; read modes never mutate.

## Bound the enforcement

- Adapt these templates as part of the project rather than making enforcement its own project.
- Any review round about the apparatus (not about work content) beyond the second means process is now policing process. Ship.
- Reconcile machinery after the delivered thing; machinery completeness does not gate delivery.
