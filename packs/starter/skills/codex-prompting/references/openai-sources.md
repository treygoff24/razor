# OpenAI sources and provenance

Open this before changing a model claim in the skill. Each line says whether OpenAI said it or common practice decided it. Pages read October 2026; OpenAI revises them, so re-read before quoting.

## Models

`sol`, `terra`, `luna` and `astra` are Razor's short names for OpenAI models (Razor's models page lists the full ids). OpenAI's pages cover `sol` and `astra`: both take the efforts `low`, `medium`, `high`, `xhigh` and `max`, `sol`'s default is `medium`, and neither takes `none` or `minimal`. Nothing read covers `terra` or `luna` separately; treat their prompting as the same contract, unverified.

## What OpenAI says (the skill adopts it)

| Claim in the skill | Source |
| --- | --- |
| Clear assignment = result, audience, context, constraints, what counts as done; Codex four-part prompt (goal, context, constraints, done when) | S2, S6 |
| Overly specific guidance and elaborate recipes now hinder; models handle nuance and ambiguity | S2, S4 |
| `astra` asks more and stops earlier; autonomy, requests-imply-action and approval-last prompts | S1, S4 |
| Define done to include implementing, running, inspecting, fixing failures; a required review stop pulls toward stopping early; name decisions needing review | S2, S4 |
| Replace blanket "always ask" rules with explicit boundaries; strong boundary language can make `astra` stop where work could continue | S2, S4 |
| `astra` is more sensitive to skills and `AGENTS.md`; audit them for conflicts; user instructions outrank skills (prompt block) | S1 |
| `astra` tests thoroughly unprompted; old "run tests" lines cause unnecessary testing; testing-calibration prompt; permit safe local test runs | S1, S4 |
| Default style is detailed and formatted with recurring phrases; style prompts | S1 |
| `astra` starts fewer subagents than desired; subagent prompt | S1 |
| Effort bands: low routine, medium judgment, high difficult debugging or careful review, xhigh/max only when high falls short and the gain pays; effort is a tuning knob, not the way to recover quality | S2, S7 |
| `none` and `minimal` unsupported on `sol` and `astra`; low is the floor | S1, S8 |
| Final handoff = what changed, what was checked, what still needs attention | S2 |
| Do not require reading a doc stack before every edit; point to docs by case | S4 |
| `AGENTS.md`: layered, nearer file wins, 32 KiB combined default cap (`project_doc_max_bytes`) | S5 |
| `sol` shows better respect for user intent and restrictions than the model it replaced | S3 |

## Common practice (OpenAI's docs are silent)

- The minimum-contract field names beyond the four OpenAI names (Output, Stop rules, Validation), the helper-brief checklist, and the final-answer wording beyond S2's three items.
- Every XML block under "Common-practice blocks" in [prompt-blocks.md](prompt-blocks.md), and the XML tag form itself. OpenAI's prompt-engineering page (S9) allows Markdown and XML delimiters but publishes its prompts for these models as plain text.
- Putting the authorization and done lines in every unattended-helper brief for `sol` too. OpenAI's behavior guidance was observed on `astra`; the pages never state that `sol` shows the same stopping behavior.
- Adding the `astra` fixes to a `sol` prompt only after a run shows the behavior. OpenAI says only to evaluate the prompts on your model.
- Pairing the scope line in `action_safety` with the list of actions allowed without asking.

## Read and not adopted

- The agentic-task sample in S9 ("plan extensively ... reflect extensively", "keep going until the query is completely resolved") is older text in a page that now names the current models. It conflicts with S2 and S4 (no itineraries; define done), so the skill uses the S1 autonomy prompts instead.
- S9's "GPT models are like a junior coworker needing precise step-by-step instructions" contradicts S2 and S4 for the current models; not adopted.
- The reasoning-effort table in S7 and the `phase` section name earlier models; the effort bands are corroborated by S2 and used, `phase` is left out.
- S10 (the Codex models page) still listed earlier models; nothing taken from it.
- S11 (reasoning best practices) covers o-series versus GPT models; not applicable.
- API-only features in S1 (async tool calling, mid-turn steering, configuration updates, fast modes, cache TTL, removing `temperature` and `top_p` at non-`none` effort): they change an API call, not how a task prompt is written. One API fact worth knowing: tool calling needs the Responses API.
- The slop-word block is adopted verbatim for use on request, though it is a list of prohibitions; the style block above it is the positive form.
- Third-party guides surfaced in search; none are OpenAI primary sources and none were used.

## Could not confirm

- Any prompt-level difference between `sol` and the model it replaced: S1's migration section covers API parameters only; S3 reports benchmarks and alignment evaluations, not prompt guidance.
- Per-model prompt guidance specific to `sol`, `terra` or `luna`: S1 states the prompts "address behavior observed with" `astra`.
- Whether the Codex CLI serves `sol` and `astra` the base instructions in S12 (the file read is on `main`); it is cited only for "the CLI already asks for short pre-tool updates and persistence".
- OpenAI's default effort for `astra`: the model page lists supported values but no default.
- A verbosity control, a "tool preamble" switch, or a migration section on prompts for these models: none found in the pages below.

## Sources

| # | URL | Subject |
| --- | --- | --- |
| S1 | https://developers.openai.com/api/docs/guides/latest-model | OpenAI's guide to the latest model: prompting best practices, migration quickstart |
| S2 | https://openai.com/index/practical-guide-building-gpt-6/ | OpenAI's model guide for the current family, 2 October 2026 |
| S3 | https://openai.com/index/introducing-gpt-6-1-sol/ | Introduction of the `sol` model |
| S4 | https://developers.openai.com/blog/rethinking-skills-and-prompts-for-gpt-6-astra | "Rethinking skills and prompts" for `astra` |
| S5 | https://developers.openai.com/codex/guides/agents-md (now redirects to https://learn.chatgpt.com/docs/agent-configuration/agents-md) | Codex `AGENTS.md` layering and cap |
| S6 | https://developers.openai.com/codex/learn/best-practices (redirects to https://learn.chatgpt.com/guides/best-practices) | Codex best practices |
| S7 | https://developers.openai.com/api/docs/guides/reasoning | Reasoning effort |
| S8 | https://developers.openai.com/api/docs/models/gpt-6.1-sol and https://developers.openai.com/api/docs/models/gpt-6-astra | Model pages: supported efforts |
| S9 | https://developers.openai.com/api/docs/guides/prompt-engineering | Prompt engineering guide |
| S10 | https://developers.openai.com/codex/models | Codex models page |
| S11 | https://developers.openai.com/api/docs/guides/reasoning-best-practices | Reasoning best practices |
| S12 | https://github.com/openai/codex/blob/main/codex-rs/protocol/src/prompts/base_instructions/default.md | Codex CLI base instructions |
