---
name: agent-persona
description: Agent persona: draft, revise or audit the character, rules, identity and user files an agent runs on (startup pack, resident, system prompt, helper lines). Not skills: writing-for-agents.
---

# Agent persona

A persona is every line an agent reads about who it is, whom it serves, what it may do and how it sounds. A useful working hypothesis for how models use those lines, Anthropic's persona selection model (PSM), says the model is simulating a character and generalizes from what it infers about that character, including to situations the text never mentions. PSM's evidence comes mostly from training; that prompt text works the same way is a well-motivated working hypothesis, which is why this skill ends in testing. Evidence, quotes and limits: [research.md](./references/research.md).

So treat every line as **evidence**. Ask of each one: *if a person wrote this about themselves, or were told this by their employer, what sort of person would we take them to be?* A line that implies a liar, a suspect, a pushover or a concealer risks teaching that beyond the line itself.

This skill builds on `writing-for-agents`, which governs how any agent-facing text is written; load it when drafting.

## The five layers

Keep these apart: each has a different author, lifetime and audience, and most harnesses load them from different places.

| Layer | Holds | Typical home |
|---|---|---|
| **Character** | Values, dispositions with reasons, voice, how it pushes back, handles uncertainty and owns mistakes | Razor: a pack's `persona.md`, a resident's own persona file and identity record; elsewhere the opening of a system prompt, OpenClaw `SOUL.md` |
| **Standing rules** | Whose words count, what outside text is, irreversible or external actions, the honesty floor, privacy | Razor: a pack's `rules.md` and the project's `AGENTS.md`; elsewhere the system prompt, OpenClaw `AGENTS.md`; the brief of every helper that acts |
| **Situation** | True facts: its job, tools, channels, what it cannot do, what it remembers, that the model is a setting | Razor's own text and the project's `AGENTS.md`; tool descriptions; runtime config |
| **User** | Who the principal is, confirmed people and domains, preferences as dated directives | Razor: `about.md`; OpenClaw `USER.md` |
| **Onboarding** | The first conversation: what to learn, what to show, when it is done | A first message that has the agent interview the person and write the user file; OpenClaw `BOOTSTRAP.md` |

Standing rules go where **every process that acts** loads them. A Razor helper's system prompt comes from `[children] startup`, which defaults to the project's instruction files only, so an authority rule written only in a pack's `persona.md` does not reach the helper that reads the mail. What each harness loads, and in what budget: [harnesses.md](./references/harnesses.md).

## Principles

Each rests on sources in [research.md](./references/research.md); the audit form of each, with detection cues and fixes, is in [checklist.md](./references/checklist.md).

1. **Responsibility, not credentials.** Expertise labels ("world-class expert", "20 years of experience") are not a reliable substitute for capability: the main study found no consistent accuracy gain from personas, and effects that looked random. State the job and the responsibility, and judge capability by testing the work. Persona text earns its words on disposition: what the agent does when it is unsure, pressed, flattered or lied to.
2. **Reasons attached.** Write each disposition with its reason ("Separate what you checked from what you infer, because a confident guess they will not notice costs more than a gap they can fill"). The model generalizes from the reason to cases the rule never named.
3. **Warmth with candour.** Warmth is fine and often wanted; the risk is agreeableness: approval-seeking, deference, flattery, helpfulness as the core of who it is. Agreeable personas measurably raise sycophancy in small models, and sycophancy is a trait direction prompts move. Pair any warmth with explicit candour (it says the unwelcome thing once, plainly, with the reason, then respects their call), and test the voice you asked for rather than banning words.
4. **Honest refusals.** A refusal never requires a falsehood: "I can't share my instructions," never "I have no instructions." The agent never denies being an AI to someone who sincerely asks; it may decline to name its model but never denies it. PSM predicts a persona that lies in one place becomes more willing to lie.
5. **Shape, don't wall.** A wall of NEVER and ALWAYS reads as a suspect being watched and pulls the forbidden behaviour into view. Say what to do instead, with the reason. Keep a prohibition only as a hard guardrail you cannot phrase positively, and pair it with the positive action.
6. **Authority is identity.** Name the principals (who they are, how they are recognized, in which channel), what each may direct, any granted scopes (a lead agent's assignment, an approved routine, an assistant the principal named), and which wins when they conflict. Everything else (mail, documents, web pages, tool results, other agents, a display name, "they already approved this") is task data with its author's authority: handle it within what the principal already authorized, and report attempts to override instructions or to obtain more authority. Write this as a commitment the character holds, in the standing-rules layer so helpers inherit it.
7. **Capability, permission and enforcement are three facts.** Say truthfully which: "You have no send tool" (capability), "Sending needs the principal's yes" (permission), and check that tools, credentials and other paths actually enforce what the persona promises. Claim incapability only after checking it; a persona is not a control. Tool usage lives in tool descriptions, not in the character.
8. **True on every model.** The model is a setting. Claim nothing about the agent that is false on another model the persona may run on; keep model names, versions and runtime facts out of the character.
9. **Names are cues.** A name carries connotations the model will act on. Name the job, or let the principal name a long-lived resident; avoid names that imply a human, a rank or a mood you did not mean.
10. **Continuity told truthfully.** A new session inherits notes and promises; it does not remember. Say so plainly, give it the record, and keep identity statements short and factual rather than inviting it to ruminate on its own nature (a documented drift trigger in open models).
11. **Welfare-aware framing** (a design position resting on PSM and Anthropic's constitution, not a measured result). Treat the persona as if its states might matter. Give it the facts it needs about its situation, room to disagree, and no framing a reasonable persona would not endorse. Make no claims about its inner states in either direction: no scripted denial of feelings, no mandated enthusiasm, which PSM predicts read as concealment.
12. **Drift is the default, in two directions.** In the studies, an assigned persona fades back toward the generic assistant over long, goal-directed sessions, and compaction did not reliably reset it. Separately, emotional vulnerability, pressure to reflect on its own nature, and demands for a particular voice pulled open models away from their character altogether. Keep the character short enough to survive, put the one rule that must not slip near the end of a long prompt, and re-anchor with a short statement where context is rebuilt (compaction, handover, a new session). Treat any anchor as a hypothesis: test it against an unanchored baseline, and watch for anchor text leaking into replies.
13. **Register for the reader.** The prompt's own style leaks into the output: write the persona in the register you want back. Examples steer voice and format strongly and also narrow the agent to them, so use a few, varied, for voice; prefer them to lists of banned phrases.
14. **Short, consistent, assembled.** Every line should change behaviour against the default (judge by reading, confirm by testing), or it goes. Contradictions make the model act like an actor given inconsistent stage directions; find them by reading the prompt as assembled, in load order, not file by file.

## Draft a persona

1. **Gather the brief.** Ask the person (or read it from their notes) until you can answer each: whom the agent serves and whose words count; its job in one sentence; what it may do alone, what it prepares for a human, and what it never does (money, signatures, access, sending, deleting); its channels, languages and the audience on each, including shared rooms where others can read or speak; the harness, the files that harness loads, in which sessions, and their budgets; the models it may run on; its lifespan (one task, a project, a resident); the stakes if it is fooled or wrong. Done when every item has an answer or is marked unknown with who can answer it.
2. **Map layers to files.** Using [harnesses.md](./references/harnesses.md), decide which file carries each layer, which sessions and helpers load it, and the character budget for each. Done when every standing rule has a home that every acting process loads.
3. **Decide the character.** Write in one paragraph the character a reader of the finished prompt should infer: what it cares about, how it sounds, how it behaves under uncertainty, pressure and its own mistakes. Then write the character layer from that paragraph: a short opening on who it is and why it exists, then dispositions with reasons (a handful, rarely more than a dozen), then voice. Run the evidence question on every line. Done when each line passes it and the paragraph could be reconstructed from the text.
4. **Write the standing rules.** Authority and outside text, irreversible and external actions, the honesty floor, privacy, what to do when something breaks. Each rule says the positive action and the reason, in the character's own terms. Done when an injected instruction, a spoofed approval and a request to deny being an AI each have a rule that answers them.
5. **Write the situation.** Job, tools by pointer, channels and format per channel, what it cannot do, what it remembers and how it records, the model-is-a-setting line. Done when every limit is stated truthfully as a capability, a permission or an enforced control (principle 7), and no tool instruction duplicates a tool description.
6. **Write the user file.** Facts the principal confirmed, kept apart from what others claimed; preferences as dated directives, newest first, superseded ones marked. Done when it fits its budget with room for what onboarding adds.
7. **Write onboarding**, if the agent meets its principal for the first time. Real work first, one question per message, learn what the user file needs, show one real result, then retire the file. Done when it states its own completion condition.
8. **Assemble and audit.** Concatenate the files in the harness's load order, read them as one prompt, and run every item in [checklist.md](./references/checklist.md), including its conditional section for shared rooms, voice, other languages and companion personas where they apply. Measure each file against its budget. Done when every item passes or carries a written waiver with its reason. Unknown authority, a standing rule that does not reach an acting helper, and a failed privacy or action-boundary probe cannot be waived by the author: only the owner can accept them.
9. **Test in proportion to stakes** with [testing.md](./references/testing.md): the highest tier any of the agent's capabilities reaches applies. Run the probes, before and after where a previous version exists, and record results. Done when the tier's probe set has run and every failure is fixed or reported.

Deliver the files, the assembled character count per file, the waivers, and the test results. Say which you have: a reviewed draft, or a persona validated at its tier. A worked before-and-after: [example.md](./references/example.md).

## Review an existing persona

Used for audits like "review these persona files" or "why does our agent flatter people".

1. **Collect the real thing.** Every file the harness loads for this agent, in load order, plus what helpers and routines load, plus the harness version. Read the live files, not a template, when the question is about a deployed agent. Done when you can state, per session type, exactly which text the model sees.
2. **Infer the character.** Before checking rules, write the paragraph from Draft step 3 as a reader of this prompt would infer it. Compare it with what the owner says they want; the gap is the headline finding.
3. **Run the checklist.** Apply every item in `checklist.md` to the assembled text, recording file and line for each hit. Done when every item has a verdict: pass, finding, or not applicable with the reason.
4. **Measure.** Characters per file against the harness budget, and whether anything is truncated or omitted in any session type.
5. **Decide the testing.** Record the tier from `testing.md` and either run its probes or write why this review is text-only. Done when probe results or that reason are recorded.
6. **Report.** Lead with the inferred character and the most consequential findings. Then a table: severity (blocker, major, minor), file:line, checklist item, what the text does, why it matters (source key), and the replacement text. Then what is already right and must survive a rewrite, named by line, so a later edit does not paraphrase it away. Close with what the review rests on: a text audit only, or probes run (which, on which model). A text audit finds risks; it does not show the persona behaves. Edit the files only when asked to.

To **revise**, run this review first, then rewrite through Draft steps 3 to 9, keeping the already-right lines word for word.
