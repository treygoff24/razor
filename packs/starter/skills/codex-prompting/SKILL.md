---
name: codex-prompting
description: Prompting GPT models (sol, terra, luna, astra) or Codex: write or revise a task prompt or helper brief for implementation, diagnosis, review or research. Not skills or AGENTS.md: writing-for-agents.
---

# Prompting GPT models

Prompt `sol` and `astra` like a senior colleague: the outcome, constraints, evidence, and what done includes. OpenAI's own guidance is that detailed itineraries now hurt where they once helped, so give steps only where the order is itself the requirement. Provenance of every claim below: [openai-sources.md](./references/openai-sources.md). OpenAI's guidance covers `sol` and `astra`; `terra` and `luna` get the same contract, and you add blocks when a run shows the need.

## Minimum contract

```text
Goal: the user-visible result.
Context: only the files/facts needed.
Constraints: scope (what may be read or changed), standards, explicit non-goals.
Done when: what must be true before the final answer, including running the result, inspecting it, and fixing what fails.
Validation: the named commands, sources, screenshots, or checks.
Output: format and brevity.
Stop rules: when to ask, search, retry, escalate, or stop.
```

## What to know about these models

- **Done includes the run.** `astra` stops sooner than earlier models and returns for review with work left. State the full done state, and name the decisions that need the reader's review; a "stop for review after the first implementation" line pulls it to stop there.
- **Authorize the work.** `astra` asks when intent is unclear. For an unattended helper say the work is approved, and say that approval comes last: finish the authorized work, then ask over a concrete, reviewable result. Blocks in [prompt-blocks.md](./references/prompt-blocks.md).
- **Boundaries as lists.** Name what may proceed alone and what needs approval. A blanket "always ask first" or "never without permission" is taken at full strength and stalls the run.
- **One authority order.** The prompt outranks skills and `AGENTS.md`. `astra` follows repo instructions closely, so read the target repo's `AGENTS.md` and skills for rules that contradict the brief or demand extra testing and docs-reading, and resolve the conflict in the prompt.
- **Size verification.** `astra` tests thoroughly unprompted, so "run tests" lines now over-test. Name the checks the change needs and say to stop there for small reversible changes; "run all tests" and "read every doc first" burn time.
- **Say how to write the answer.** The default is long, formatted, with recurring phrases. Give the output shape and register.
- **Say when to use subagents.** `astra` starts fewer subagents than a workflow often wants; give the condition and the limit.
- **`sol` is unproven on the `astra` fixes.** OpenAI derived the behavior guidance from `astra` and says to evaluate it on your model; `sol` follows user intent and restrictions better than the model it replaced. Add the blocks when a `sol` run shows the behavior, not by default. A brief for an unattended helper gets the authorization and done lines either way.

## Reasoning effort

- `low`: routine, well-scoped work (extraction, small edits). `sol` and `astra` have no `none` or `minimal`, so `low` is the floor.
- `medium`: planning, comparing options, work needing judgment. The `sol` default; start there.
- `high`: difficult debugging, deeper analysis, careful review.
- `xhigh` / `max`: when `high` falls short and the gain justifies the time and cost. Never as a patch for an unclear prompt: clarify the contract first.

The effort is a setting of the session (`/effort`, or the `effort` field when you start a helper), not a line in the prompt.

## Helper briefs

A helper started with the `agent` tool (`action: start`, `model: sol` or another GPT alias, `effort`, `kind`, `placement`) sees only its `brief`, so the brief carries the whole contract above plus:

- the exact owned files or the read-only scope,
- what the helper decides alone and what it must stop for,
- the expected report shape (its `agent finish` report) or artifact path,
- the validation and review it owes,
- whether it may edit or only draft.

Mixed jobs go to separate helpers (review, fix, docs, strategy), unless the person wants one bundled pass.

## In the Codex CLI

Codex reads `AGENTS.md` files layered from its home folder down to the working folder; a file nearer the working folder wins, and the combined text is capped at 32 KiB by default. Put standing rules there, the task in the prompt, and keep the two from contradicting each other.

## Evidence

Use current sources for unstable facts, laws, APIs, SDKs, prices, schedules, and named people and company roles; inspect live files for code. Say what is confirmed and what is inferred. For plans, map requirements to files and systems, risks, validation, rollback, and open questions.

## Final answer standard

Return what changed or was found, what was verified, what remains risky or blocked, and the next best step. Keep it as short as the task allows.

## Reference files

- [prompt-blocks.md](./references/prompt-blocks.md): paste-in blocks, OpenAI's published prompts first, common-practice blocks after. Open it when a prompt needs a section spelled out.
- [codex-prompt-recipes.md](./references/codex-prompt-recipes.md): whole prompts for diagnosis, a narrow fix, a long autonomous task, review, research, and patching an existing prompt. Open it when the task is one of those.
- [codex-prompt-antipatterns.md](./references/codex-prompt-antipatterns.md): bad-versus-better pairs, with a legacy-prompt audit. Open it when revising a prompt that underperformed or carries over from an earlier model.
- [openai-sources.md](./references/openai-sources.md): what OpenAI says versus common practice, what was not adopted, what is unconfirmed, and the sources. Open it before changing a model claim.
