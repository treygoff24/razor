# Harness notes: what each one loads, where, and in what budget

Harnesses change: re-read the installed version's docs before relying on a number here, and prefer reading the rendered prompt over this page.

## Razor

Razor's own docs (`configuration.md`, `always-on-agents.md`, `troubleshooting.md` in the docs folder beside the `razor` program; the `razor-settings` skill says where) are the source for this section.

**The startup list** is the system prompt of a top-level conversation, in order. Three words are not files: `razor` is Razor's own text about its tools, `project` is the `AGENTS.md` and `CLAUDE.md` files from the repository root down to the session's folder, and `about` is `~/.razor/about.md`. Any other entry is a file, or a folder whose first 64 `.md` files are read. The starter pack's list is `["persona.md", "razor", "about", "project", "rules.md"]`: `persona.md` and `rules.md` are files beside the pack's `config.toml`, and `rules.md` comes last, the slot that suits the one rule that must not slip.

- **Changing it.** `startup` and `[children] startup` are the person's own file's to set (or a pack's), never a project's. Copy the whole list into the person's `config.toml`, because a list replaces and never joins, and copy `persona.md` and `rules.md` beside that file: a relative path counts from the file naming it, and `~/` from the home folder. Do not edit the pack's own copies; an upgrade replaces them.
- **A resident** (an always-on agent) starts with the person's list unless its `[residents.<name>] startup` gives its own, which replaces the whole list; give it a persona by naming a file in it.
- **A resident's identity record**, `identity.toml` in its session folder, is made from a template at its first start with its display name, role, values, relationships and ways of working. Every conversation it has starts with it, and the resident keeps it current itself with its file tools. It is the agent's own character text, loaded beside the persona file: read both together, and keep them from contradicting each other.
- **Helpers** get `[children] startup`, default `["project"]`, independent of the parent's list. Their system prompt holds the project's instruction files and none of `persona.md`, `rules.md` or `about`, unless the person adds them. Anything that must bind a helper goes in the project's `AGENTS.md`, or the person adds a rules file to `[children] startup`. A helper started as a reviewer or researcher also gets Razor's instructions for that role; `[children] reviewer` and `researcher` can replace them with a file.
- **Rebuilds.** A resident's night, `/handover`, a full context and some restarts rebuild a session's context from its record: a fresh context with the person's latest requests, a continuation note and a list of what happened since, not the old turns. Razor's docs say helpers' startup files are read again at a rebuild; check a rebuilt session's `startup.md` to see that the persona text is in it, and keep the persona in the startup files rather than in the chat, since the chat does not carry over.
- **The assembled prompt.** Each session's folder holds `startup.md`, exactly what the conversation was sent at its start (`/startup` shows it in the terminal app). Audit that, not the loose files.
- **Budgets.** Razor's docs state no size limit for startup files (a folder entry reads its first 64 `.md` files). The budget is the model's attention: keep persona, rules and `about.md` short enough to read in one sitting, and measure the assembled `startup.md`.
- **Testing.** `razor --print` runs a message non-interactively in the current folder with the startup list loaded as in production; `--model` and `--effort` choose how it runs, `--continue` and `--resume` carry a conversation. A second home (`RAZOR_HOME` pointing at another folder) keeps probes away from the person's real sessions and memory; a GPT sign-in is per home, Claude's is shared across them.

## Claude Code (docs at code.claude.com)

- **CLAUDE.md** (user, project, local, managed, and `AGENTS.md` files loaded as project instructions) arrives through the message flow, not the system prompt. Anthropic's SDK docs: "Instructions in a user message carry marginally less weight than the same text in the system prompt." A persona section in CLAUDE.md works but carries slightly less weight and competes with everything else in that file; keep it short and put it first.
- **Output styles** change the system prompt itself: role, tone and format for the whole session. A custom style drops Claude Code's software-engineering instructions unless its frontmatter sets `keep-coding-instructions: true`. This is the strongest place for a Claude Code persona; it applies to the main conversation and forks, not to other subagents.
- **Subagents**: "a custom subagent's system prompt replaces the default Claude Code system prompt entirely". The markdown body is the whole character and operating guide, plus environment details Claude Code appends. CLAUDE.md still loads unless the definition sets `omitClaudeMd: true`. The `description` field decides when the main agent hands work to it, so it is a context pointer, not a persona line.
- `--append-system-prompt` adds to the system prompt without removing anything; `--system-prompt` replaces it and you then own every tool and safety instruction the agent still needs.

## OpenClaw (read from the 2026.9.8 package docs)

| File | Loaded | Role | Budget |
|---|---|---|---|
| `AGENTS.md` | Every session, **and the only bootstrap file sub-agents get** | Operating guide: standing rules, how to work, memory rules, the `## Tools` section (local tool conventions; it does not grant tools) | 20,000 chars per file by default (`agents.defaults.bootstrapMaxChars`), truncated beyond |
| `SOUL.md` | Every normal session; not sub-agents | Voice, values, stance | Same per-file limit |
| `IDENTITY.md` | Every normal session; not sub-agents | Name, vibe, emoji; written during onboarding | Same |
| `USER.md` | Every normal session; not sub-agents | The principal: profile, relationships, preferences as dated active/superseded directives | **Fixed 4,000-character cap** the config cannot raise |
| `BOOTSTRAP.md` | While it exists and setup is not complete | The first-run ritual; the agent deletes it when done | Same per-file limit |
| `MEMORY.md` | Main, private session only | Curated durable facts and decisions | Same |
| all of the above | | | 60,000 chars total (`agents.defaults.bootstrapTotalMaxChars`) |

For the persona author:

- **Anything that must bind helpers goes in `AGENTS.md`.** "Sub-agent context only injects `AGENTS.md` (no `SOUL.md`, `IDENTITY.md`, `USER.md`, `MEMORY.md`, or `BOOTSTRAP.md`)". Authority, outside-text, privacy and send rules written only in `SOUL.md` are invisible to the helper reading the inbox. If helpers need user facts (real domains for a fraud check), `AGENTS.md` tells them which section of `USER.md` to read, and only that.
- **`USER.md` past its cap may not arrive whole.** Whether an oversized file is truncated or omitted varies by version; write memory rules that keep it under the cap (merge, prune, supersede) and leave room for what onboarding adds.
- OpenClaw's own SOUL.md guide aims at a real problem (a bland, hedging assistant voice) with blunt instruments: "strong opinions", "commit to a take", mandatory brevity. Take its diagnosis, not its prompt: "commit to a take" without "and say how sure you are" trades hedging for overconfidence. Keep its sound split: "Keep `AGENTS.md` for operating rules; keep `SOUL.md` for voice, stance, and style."
- Check the rendered context, not the files, for what a session saw: `agents.defaults.contextInjection: "continuation-skip"` skips re-injection on continuation turns.

## Subagent briefs (any harness)

The identity part of a brief is one or two sentences: the job (a responsibility, not a flavour: "you review this diff for security defects", not "you are a meticulous senior engineer"), the principal it reports to, and that its report is information for that principal. Then Anthropic's contract for the rest: "an objective, an output format, guidance on the tools and sources to use, and clear task boundaries." Name its siblings' jobs when there are siblings, so work divides; avoid group labels (model family, team colours) that can split agents into factions. A reviewer may be warm, never approval-seeking: no traits that reward agreeing with the author.

## API system prompts and specialists in a product

- The product's own system prompt is where persona effort pays most: Claude Code's team cut over 80% of their own prompt with no loss, but says a harness builder's system prompt "is where you should spend a lot of time" (research.md, [CTX5]).
- Specialists in a multi-agent product: each gets its responsibility, scope as fact, its honest-negative deliverable ("report what you could not establish"), and an output contract in one line. The coordinator, which talks to the user all day, needs the real character; it is often the least-written prompt in the product.
- The prompt is assembled from several sources (base prompt, memories, retrieved context, component catalogues). Audit the rendered prompt, and check what lands last: if a catalogue is appended at the end, the late-prompt reminder slot is spent on it.
- For GPT-family models the cost of a contradiction is reasoning spent reconciling it (OpenAI's prompting guide for an earlier GPT model); a persona meant to be model-agnostic should be contradiction-free for that reason too.
