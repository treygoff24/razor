---
name: repair
description: Repair a bug, regression, flaky test, race, leak or wrong state: name the broken commitment, build a loop that goes red on it, find the cause, make the smallest sound fix. Load before any fix.
---

# Repair

A fix restores a commitment the product made. The symptom is where to start looking; the commitment is what you are restoring; the cause is what you change. Every step ends on something you can check.

## 1. Name the violated commitment

Write down expected versus observed behavior and the conditions under which it fails. Name the commitment the observed behavior breaks: a spec line, a rule, a user-visible promise, a test. Say what would falsify your first guess at the cause.

**Complete when:** expected, observed, conditions and the commitment are written in one place, and a falsifier is named.

## 2. Build a loop that goes red

Most of the work is a tight pass/fail signal that goes red on *this* bug: with one, bisection, hypotheses and instrumentation all just consume it; without one, reading code will not find the cause. Ways to get one, roughly in order: a failing test at whatever seam reaches the bug; a curl or HTTP script against a running dev server; a CLI run on a fixture input diffed against a known-good snapshot; a headless browser script asserting on DOM, console or network; a captured trace or payload replayed through the code path in isolation; a throwaway harness that runs only the failing path; a property or fuzz loop for "sometimes wrong"; a `git bisect run` harness between two known states; a differential run of old against new; last, a scripted human-in-the-loop step.

Then tighten it: faster (skip unrelated setup, narrow the scope), sharper (assert the specific symptom, not "didn't crash"), deterministic (pin time, seed randomness, isolate filesystem and network). For a flaky bug the goal is a higher reproduction rate (loop it, parallelise, add stress), not a clean repro.

Keep credentials out of what you capture: commands, logs, traces and fixtures carry secrets and auth headers, so redact at collection, keep secrets in the environment, and quote only the lines that carry the signal.

**Complete when:** you can name one command you have already run that goes red on the bug. While a loop stays out of reach, continue the authorized investigation in step 3 from traces, source and controlled experiments; say what you tried, name the evidence or access that would unblock it, and carry the repair as unproven until a loop exists.

## 3. Find the cause

Trace the state, effects, ownership and data flow behind the symptom, and say where the trace is incomplete. Check recent changes (`git log`) and whether the same pattern lives elsewhere: one function or a subsystem, one input or all, one environment or all. Form a hypothesis that states the mechanism, then run the experiment that distinguishes it from the alternatives (sync the clocks; widen the tolerance; log the delta) and let the loop judge.

Classify what you found; several can coexist:

- a local defect: the code does the wrong thing here;
- a violated invariant: two places disagree about a fact the system assumed was one;
- an outdated model: the domain changed and the code's picture of it did not;
- an environment assumption: a timeout, path, clock, locale or resource the code took for granted;
- a wrong requirement: the commitment itself is mistaken or contradicts another.

**Complete when:** you can say *why* it fails, not only where, and an experiment has confirmed the mechanism. A diagnosis-only request ends here, reported in the shape of step 5.

## 4. Choose the smallest sound fix

Candidates, in the order to consider them: fix the fact at its source of truth; delete the redundant mechanism that let two sources disagree; a local fix. Sound beats small: a one-line patch that leaves two sources of the same fact is a second bug waiting. A rewrite is never required when a surgical fix is sound.

If the fix you are reaching for adds a flag, a mode, a retry, a cache, a fallback, a special case, a compatibility path, or a second copy of a fact, apply the rule in `changing-code`, "Mechanisms that need a reason": it earns its place by the obligation it serves and the comparison with the simpler alternative, or it names its retirement condition and owner, or the change stops and loads `task-design` or `challenge-the-design` for that part. The classic: "the UI says stopped when it isn't, so add a second status flag." The sound fix asks whether one authoritative lifecycle can produce the right state.

Take the fix to the person before making it when it changes a product promise, a protected architectural commitment, a performance guarantee, or an approved size budget.

**Complete when:** the chosen fix names the cause it removes, and every mechanism in it from the list above has its reason or its retirement condition written down.

## 5. Verify and report

The loop from step 2 goes green; the related cases named in step 3 hold; nothing adjacent went red. Keep the loop as a regression test where a seam allows; remove temporary instrumentation, debug logging and scratch harnesses. Verification standards are `no-ceremony`'s: what ran, what it establishes, what remains unverified.

Report in the shape `changing-code` sets, plus what only a repair knows: the cause's classification, whether the loop was kept as a test, and, when the subsystem carries a size budget, the line delta and remaining headroom. A fix whose failure was never reproduced is reported as unproven, with the reason. Record a note beyond the commit only when the finding is reusable or points at a deeper structural issue, in which case name it as a candidate for `challenge-the-design`.

**Complete when:** the repair passes its discriminating check and the report carries cause, change, evidence and remaining uncertainty. An unresolved failure ends as an investigation report and leaves the repair open.
