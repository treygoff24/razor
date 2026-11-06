---
name: cross-family-review
description: Cross-family review of a finished multi-file package (memo, annotated legal set, outreach batch, model) before it ships: reviewer helpers from a family that wrote none of it, then written triage.
---

# Cross-family review

Reviewer helpers attack a finished package, each chosen on two axes, and the coordinator (you) dispositions every finding in writing. It is for a multi-file deliverable about to reach its audience. One change in a repository goes to the `review` skill; code structure goes to `structural-review`.

- **Decorrelation.** An author's own model family shares its blind spots, so every helper comes from a family that wrote none of the content. Independent helpers from another family converge on defects the author cannot see, cross-file contradictions among them.
- **The crew.** A helper is a discipline, chosen from what the package *does*, doing what that discipline does with that discipline's instruments. An auditor reconciles every figure to its source; a short seller hunts the one claim that breaks the thesis; a hostile counsel reads for the admission. The brief names the method and the evidence to return. A helper asked to *be* a short seller writes in a voice; a helper asked to *do what a short seller does* brings the weak claim with the source that undercuts it.

**Convergence**, independent helpers surfacing the same defect, is the high-confidence signal: a converged finding is fixed, full stop.

## Steps

1. **Say what the package does, in verbs.** Not "a deck" but "gets a committee to believe a causal story and decide under uncertainty in thirty minutes"; not "an annotation set" but "lets a staffer trust each change without reading the statute". Name the real reader. **Done when** one sentence of verbs and one named reader exist.

2. **Assemble the crew.** From the verbs, pick two to four disciplines whose method finds what the author cannot see. [AXES.md](AXES.md) gives a starting crew per package type (annotated legal, policy memo, outreach batch, finance model, generic); adapt it to the verbs. For each helper write its discipline, its method, its instruments (a reconciliation against the source table, a grep for every figure, a read of the cited record) and the evidence it must return. **Done when** every helper is a discipline with a method, instruments and an evidence form, and no brief says "act as".

3. **Pick families by authorship.** Razor's model names cover two families, Claude (`opus`, `sonnet`, `haiku`, `fable`) and GPT (`sol`, `terra`, `luna`, `astra`); `razor doctor` lists the models this install can run. Exclude the family that wrote the package; when both wrote parts, give each helper the files the other family wrote. Two helpers is standard, both on models of the other family: for example `sol` at `xhigh` and `astra` at `high` on a package Claude wrote. A *critical* package (legal, money, security, or one the person calls critical) gets three helpers: add a third family if the person has one configured, otherwise a third discipline on another model of the reviewing family, and say in the disposition that its blind spots overlap the others'. With only the author's family available, the review is not cross-family: say so before starting, and in the disposition. **Done when** no helper shares a family with the author of the files it reads.

4. **Write tripwires and name the ground truth.** Tripwires are the package's frozen claims, forbidden phrases and numbers that must not drift; they go verbatim into every brief *and* into a grep you run yourself. Attach the intent to each ("bill names for OUR bill; members' own bills are expected content"), or a literal helper floods the report. A package derived from a canonical record (sworn testimony, filed statute, executed contract, a spec) names that record in every brief with a readable path, and the standard is *consistency with the record*: a record-blind helper flags the package's most-vetted verbatim language as unsupported, burying the real findings. **Done when** the tripwire grep runs and every brief carries the record's path.

5. **Brief and start every helper at once.** The skeleton and the output contract (findings only, ranked, each with severity, file, quoted text, why and fix; a clean category said in one line; no rewriting; no praise) are in [TEMPLATES.md](TEMPLATES.md), with the `agent` start calls. A helper reads the package and writes nothing but its report; tell it so, and tell it that its folder may be a copy, so it resolves paths locally before calling anything missing. **Done when** every helper is started and every input it needs is on disk where it will look.

6. **Triage and disposition.** `wait` for the reports and merge the findings. Converged findings are fixed. Every other finding is applied or declined, and a declined one carries a written reason (audience, register, deliberate design call) in a disposition note beside the package (template in [TEMPLATES.md](TEMPLATES.md)). Fixes go to a fresh helper or your own hands, never to a reviewer. One round, then you check each finding against the fixed package yourself; a second full round is for a fix that changed the design, or a critical package. **Done when** every finding from every helper appears in the note as applied or declined with a reason.

7. **Re-verify.** Re-run the tripwire grep and the package's mechanical invariants (byte-fidelity scripts, build gates) on the fixed files. **Done when** both are clean at the revision that ships.

## Rules

- Reviewers read; the coordinator and fix helpers write.
- Review finished packages only: half-built input spends helpers on defects you already know.
- Say before running a multi-round loop: each round spends the person's model budget. A single round on work about to ship needs no permission.
