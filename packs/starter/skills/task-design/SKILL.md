---
name: task-design
description: Task design. Before designing a project's first slice, resolving open choices for a feature or behavior change, answering "how should we build X", or when a change would exceed a size budget.
---

# Task design

Design enough to make the requested work clear, then finish in the requested mode. This method does not expand the authorized scope.

## 0. Collect the project vision once

For a new project epic (a port, product build or redesign), collect this block from the person before designing its first slice. Reuse an existing approved block; a bug fix or a later feature does not restart the interview. Prefer the person's words, read supplied material first, and ask only for consequential missing answers. Interviewing at length is the `grilling` skill's job; this step stays short.

| Heading | Content |
|---|---|
| What we are building | The product or change |
| Why | The failure or opportunity it answers |
| For whom | Whose use matters and how |
| What must be true when it is done | Outcomes the person will judge, including the first look |
| What exists that this replaces or extends | Implementations, workflows and surfaces being kept or replaced |
| Stack | The tools and why; explain any deviation from the project's usual stack |

The same block heads the plan and every helper's brief, so everyone works from one vision.

**Complete when:** the block answers each field, the decisions only the person can make are named, and the plan and prepared briefs carry the same approved vision.

## 1. Establish the boundary

Classify the request as a question, review, brainstorm, plan, or implementation. Questions, reviews, brainstorms, and plan-only requests end with their requested answer or artifact. Write a file when the request authorizes that deliverable; read-only work remains read-only.

For implementation, read the relevant live files and local instructions, inspect dirty state, and extract the intended behavior, success criteria, supplied brief, and approved decisions. Approval for unchanged scope satisfies the design gate. A prior plan supplies context; the current request supplies authority.

**Altitude.** When the request names an artifact (a report, a deck, a fix for one crash) and that artifact would be discussed in a meeting, name the outcome it serves and offer the outcome-shaped request beside it: "keep the apps crash-free" where the request said "fix this crash". The request still sets the deliverable; the offer is the person's to take or leave.

**Complete when:** the mode, authorized deliverable, live constraints, success criteria, settled decisions, and held actions are all explicit.

## 2. Test the design against the real problem

Compare the design's examples with the live data model before choosing its pattern. When a screen drawn for N fields or steps now serves a seed or catalog with M far beyond N, report the outgrown design before building. Treat the pattern as evidence of intent rather than a scale contract.

Build the outcome matrix. For every state a user or record can occupy, including last, only, empty, and `other`, name the required behavior. Assign a decision owner wherever the specification is silent. Include validation: enumerate every formatted field and identify the single source for each rule.

**Depth.** For nontrivial work, answer four questions before settling the design, and fold in each answer that changes it: what a top professional in this field would do here that the request does not ask for; the rawest material they would start from, instead of what has been digested already; the specific methods and instruments they would use (a database and a statistical model where a table was offered); and what, other than the model or the person, shows the result is right. The last answer becomes the design's outside check.

**Invention.** Before adding behavior to an existing subsystem or replacing a significant mechanism in one, draw three shapes: additive (extend what is there), consolidate (an existing mechanism grows to cover the new case, or two mechanisms become one), and subtractive (delete or replace something so the feature falls out of a simpler model). For consequential work, compare the causally distinct ones against the obligations and name the evidence that selects one; for a small obvious change, one line says why consolidation and subtraction add nothing. When the stakes justify it, design once under a temporary constraint (one source of truth for every fact; no new persisted state; no new coordinator; half the lines), then relax it and keep whatever survived. Keep the new conceptual insight separate from the machinery that realizes it, and treat no incumbent detail as a protected principle unless it was approved. Whether the subsystem should exist in its current form at all is `challenge-the-design`.

**Budget.** When the change would carry a subsystem past an approved size ceiling, count the baseline and the candidate's delta the same way, and report ambiguous file ownership as unknown rather than adjudicating on it. Compare the shapes above on behavior, coordination, volume, performance, dependencies, debugging and evolution; size is search pressure, never the objective, and lines spent on necessary explicitness are good lines. Recommend one of: keep the ceiling, revise the design, reduce the scope, or ask the person to raise it, with the delta and its uncertainty. The increase is the person's to grant.

**Complete when:** every live field or step maps to a suitable interaction, every reachable state has an outcome, every format rule has one source, every unresolved outcome has a named owner, nontrivial work names its instrument and its outside check, consequential work in an existing subsystem names the evidence that selected its shape, and a budget question ends in one of the four recommendations with its counted delta.

## 3. Resolve consequential uncertainty

Proceed directly on a small, clear fix or specified change, stating any material assumption briefly. For a missing answer that changes behavior, architecture, cost, privacy, reversibility, or acceptance criteria, inspect available evidence first. Put the remaining decision to the person as one concrete question.

For significant unresolved architecture, present a recommended design, the important alternative, and the tradeoff. Continue independent authorized work while the unsettled part waits. Prefer reuse and the smallest correct change; focus review and helper work on the actual crux.

**Complete when:** each consequential choice is settled by evidence, an explicit ruling, or a named hold, and all work independent of a hold is ready to proceed.

## 4. Finish at the authorized boundary

For plan-only work, deliver a usable plan covering scope, decisions, sequence, verification, and every material open question. For approved implementation, execute within those bounds and verify changed behavior with focused checks, the repository's required gates, and meaningful tests for nontrivial logic. Stop extending the design once it supports the authorized task; surface a new material scope change for decision.

Use an existing project plan or task list when continuity requires one. Create a new document when requested or when a real handoff requires it. An action with its own authorization boundary (spend, an external send, a push to a shared or deploying branch, a deploy) still waits for the person; the approved design does not erase that.

**Complete when:** the requested artifact or implementation exists, every stated success criterion has checkable evidence, every required gate has a recorded result, and every remaining hold names its owner and prerequisite.
