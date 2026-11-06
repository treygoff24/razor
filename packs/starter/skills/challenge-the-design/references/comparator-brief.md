# The comparator's brief

The shape of the brief given in step 4 to a reviewer from the other model family than the challenger's. Fill the angle brackets; keep the seven parts and the evidence tiers.

```
Read-only. You are in a throwaway worktree of the repository; edit nothing under <source folders>. Write exactly one file, `challenge/REPORT.md`, and also print its full text as your final message.

Goal: an honest comparison of <N> independently written designs for <the subsystem> against the incumbent implementation, so the owner can decide whether anything should change. The incumbent is allowed to win. No preference for the newer design, the shorter one, the better-explained one, or the one written by a model of your own family.

What is in `challenge/`: BRIEF.md (the behavior-only brief the challengers received; they saw nothing else); DESIGN-<a>.md, DESIGN-<b>.md (frozen before any reveal; FREEZE.md records each file's SHA-256, time, model, and what each challenger could reach); DELETION-PASS.md (the coordinator's table of the incumbent's mechanisms and the promise each serves: a hypothesis to verify).

The incumbent: read <the product spec's sections>, then <the design pages>, then the source: <files>. <The product's tests> exercise this subsystem; <the git log range> shows the fix history.

Produce:
1. Smuggled formulations. Which numbered obligations in BRIEF.md are incumbent formulations dressed as behaviors (a rule a person cannot observe; a rule only one implementation needs), as opposed to approved product behavior? For each, the underlying behavior. This bounds how much the challengers could have deviated.
2. Obligation coverage. For each design and for the incumbent: each obligation met by construction, met by a check, missed, or misread, with line citations.
3. Axis comparison, each claim with its evidence tier (A: measured or exercised in this repository; B: reasoned from the source; C: the design's own prose): obligations met and missed; depth, locality and seam placement; the distinct concepts a reader must hold to answer the subsystem's central question; counted size (measured for the incumbent, estimated for each design); runtime cost on the hot paths against the budgets; debugging cost; how each absorbs the changes actually coming: <three named changes>.
4. Residue and misses. In the incumbent: mechanisms that exist only because an earlier formulation left a gap, with the commit or design section that added each; defects the brief exposes. In each challenger: obligations missed or weakened, and legitimate differences that are tradeoffs rather than defects.
5. Verify the deletion pass: confirm or correct each tag with evidence; add mechanisms it missed.
6. The ranked list: keep, simplify, replace, retire, or investigate, for the incumbent as a whole and for each mechanism or cluster where a challenger's formulation is materially simpler. "The existing design is excellent; change nothing" is a valid top result. Mark every item that would change a product promise, an architectural commitment or a size budget: those are the owner's decisions.
7. The decisive claims: the two or three claims the ranking rests on that evidence you do not have could settle (a measurement, one implemented slice, one test), and what would settle each.

Output: `challenge/REPORT.md`, under <N> lines, plain words, file and line citations for every claim about the incumbent, design line citations for every claim about a design. Say "nothing real" plainly where you found nothing. Stop when the seven parts are written.
```

Start it as a helper with `kind: reviewer`, in a throwaway worktree with the packet copied in, so the checkout stays clean and the reviewer can read the tests and the git log. Two comparators, each sealed from the other (a second model of the same family, or a challenger of each family compared by the other), make the verdict worth more than one; so does writing your own view down before reading either.
