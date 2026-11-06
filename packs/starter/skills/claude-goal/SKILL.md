---
name: claude-goal
description: Write a Claude Code `/goal` condition: paste-ready text that starts a fresh session on a job and tells its evaluator when to stop. Use for a goal to paste or a long build for an unattended session.
---

# Claude goal

`/goal <condition>` does two jobs at once. Setting it starts a turn with the condition as the directive, so the text is the kickoff prompt. After every turn a small fast model (Haiku by default) reads the conversation and rules the condition *not yet met*, *met*, or *impossible*, so the text is also the finish line. The evaluator calls no tools: it judges only what the session has printed. Every goal you write is built around **printed evidence**.

## Write the goal

1. **Point at the durable source.** Name the plan or spec by repo path and commit SHA, and the skill that runs the work. The goal carries pointers; the plan carries the detail. When the goal doubles as the person's go-ahead on a plan, say so in its first sentence and say where the session records it.
2. **Write the finish line as printed evidence.** The final message quotes verbatim the line a named command printed, names the commit it ran on, and names the artifact (a URL, a path). Read the script to take the exact success string, including any qualified variant it can print (`ACCEPTANCE OK (next 16.3.7; visual baselines pending until M1.CP)` versus the plain line). Put the evidence in the final message itself: a long run compacts, and the evaluator reads the conversation as it stands.
3. **Stop at the person's gates.** When the next step is the person's own decision (a read, an acceptance, a spend, a send), the finish line is "ready for the person", with the packet delivered in the final message. The goal ends where their judgment begins.
4. **Add a blocker exit.** Include: "Also done if a decision only the person can make blocks all remaining work: ask them, and if they don't answer, stop and name the blocker." This gives a stuck run a reported ending in place of a spin or a silent *impossible*.
5. **List what stays held.** Name only the boundaries a long run could plausibly cross: pushing to a shared remote, production, spend, secrets, external sends.
6. **Settle every open question.** Each decision the person hasn't made gets a stated default inside the goal, taken from the plan where the plan has one.
7. **Bound bounded work.** For a task with a known shape, add a turn clause (`or stop after 20 turns`); the session reports progress against it each turn. A long build with a blocker exit can run unbounded: several turns without tool use already stop the loop.
8. **Check it.** It is done when all of these hold:
   - `wc -c` is at most 4,000, counting `/goal `;
   - every path, SHA, command, and quoted string resolves (`git cat-file -e <sha>`, `rg` the quoted line in its script, `command -v`);
   - it contains no time estimates;
   - it is one code block beginning `/goal `.

## Deliver

Reply with the code block, then:

- one line per default from step 6, with the one-sentence swap the person can paste in its place;
- the launch note: start the fresh session in auto or bypass permission mode, since a goal keeps whatever mode the session has and manual mode stops at every unapproved tool call.

## How the loop behaves

Source: [Keep Claude working toward a goal](https://code.claude.com/docs/en/goal), checked against Claude Code 2.1.282.

- **One goal per session.** A new `/goal` replaces the old one. Bare `/goal` shows the condition, elapsed time, turns, token spend, and the evaluator's last reason. `/goal clear` (or `stop`, `off`, `reset`, `none`, `cancel`) removes it, as does `/clear`. Resume restores an active goal with fresh counters.
- **Background work defers evaluation.** While a subagent or background shell runs, the turn goes unjudged. After 30 minutes of waiting a check-in asks the session to read the running tasks and fix or stop stuck ones; `CLAUDE_CODE_GOAL_CHECKIN_MINUTES` sets that interval, and `0` turns check-ins and retries off.
- **Errors.** Auth failure, exhausted credit, an overflow compaction can't clear, or an unavailable model clears the goal: fix the cause and paste the same `/goal` again. Rate and usage limits pause it, and it resumes when the limit resets.
- **Headless.** `claude -p "/goal …" --output-format stream-json --verbose` runs the loop to completion and streams each turn.
- **Availability.** It follows the hooks trust rule and is off when `disableAllHooks` or `allowManagedHooksOnly` applies; the command says why.
- **Codex has its own `/goal`**, with pause and resume; this skill writes conditions for Claude only.

## Example

The first milestone of a site build. It is about 1,000 characters: pointers to the plan, a printed-evidence finish line, a gate for the person, a blocker exit, the held boundaries, and one default.

```
/goal This is the go for Milestone 1 of PLAN.md at commit 713a902. Record the go on the plan's status line, then build Milestone 1 following the plan's "Launch boundary" section: the pre-launch commits, the integration worktree, the waves with the full pnpm gate at each lane's HEAD before anything closes, then the protected preview deploy in the order the plan sets. Done when your final message quotes verbatim the ACCEPTANCE OK line that tests/acceptance.sh printed against the deployed preview, names the commit it ran on and the preview URL, and delivers the review packet the plan specifies. Also done if a decision only the person can make blocks all remaining work: ask them, and if they don't answer, stop and name the blocker. Hold throughout: no push to the shared remote, no production deploy or subdomain, no protection-bypass secret. If next 16.3.7 isn't published when the preview is ready, deploy on 16.3.6 behind the host's login as the plan allows and say so in the packet.
```
