# Skill mechanics

The skill-specific branch of [`writing-for-agents`](SKILL.md): what changes when the document is a skill (frontmatter, the invocation choice, and router skills). Everything else about writing it is the universal reference in `SKILL.md`.

## Frontmatter

Razor names a skill by its folder and reads one frontmatter key, `description`, cut at 200 characters; it ignores every other key. Keep `name` equal to the folder name for other harnesses. At the start of each conversation Razor lists the pack's and the project's skills with their descriptions; the agent reads a skill's body when the description matches the work.

## Invocation

Two choices, trading the two loads:

- A **model-invoked** skill keeps a `description`, so the agent can fire it autonomously, and other skills can reach it. You can still type its name: model-invocation always _includes_ user reach; a description only ever adds agent discovery, never removes the human's. The description is the skill's top-level context pointer, forced to stay loaded at all times: permanent context load in exchange for discoverability. A model-invoked skill whose content is all reference is also one home for shared reference: another skill can invoke it, so reference needed by several skills lives in one place. Mechanics: write a model-facing description carrying the trigger branches (the pointer-writing rules in `SKILL.md` apply in full).
- A **user-invoked** skill strips the description from the agent's reach: only the human typing its name can invoke it, and no other skill can. Zero context load, but it spends cognitive load: you are the index that must remember it exists.

Razor skills are model-invoked: every skill's description is listed, so write each one as the context pointer it is. User-invoked skills are Claude Code's mechanism, in its own skill system outside Razor: there, `disable-model-invocation: true` removes the skill from the model's reach and the `description` becomes a one-line human-facing summary. Razor does not read that key, so a Razor skill carrying it stays listed and fires like any other.

Pick model-invocation only when the agent must reach the skill on its own, or another skill must. In Claude Code, a skill that only ever fires by hand can be user-invoked at no context load.

Shared reference that two user-invoked skills both need can live in neither: with no descriptions, neither can fire the other. Push it to a plain file outside the skill system: external reference any skill can point at.

## Splitting by invocation

The invocation cut of splitting (the sequence cut lives in `SKILL.md`): split off a model-invoked skill when you have a distinct leading word that should trigger it on its own (a trigger word you actually use in your prompts), or another skill must reach it. You pay context load for the new always-loaded description, so that independent reach has to be worth it.

## Router skills

When user-invoked skills multiply past what you can remember, that piled-up cognitive load is cured by a **router skill**: one user-invoked skill that names the others and when to reach for each, so the human has one skill to remember instead of many. It can only hint, never fire them: user-invoked skills have no description, so nothing but the human can reach them. In Razor, where every skill is listed, a router earns its place only when several skills share one leading word and one pointer can name them better than separate descriptions can.
