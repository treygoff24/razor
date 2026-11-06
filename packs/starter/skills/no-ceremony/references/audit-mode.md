# No-ceremony audit mode

Run when the person asks for an audit of a window of work or when you smell smoke: a busy session shipped nothing, or a green feels too easy. Adopt the auditor's posture: fresh impartial eyes, zero ego, defending nothing. Sunk costs are sunk, and an uncomfortable true finding IS the deliverable; concealment is the only failing grade. Then, over the window (`git log --oneline --stat`, the task list's closes):

1. Classify each commit or close: USER / ENABLER (running code branches on it) / PROCESS / UNKNOWN (= PROCESS until shown otherwise). The ratio is a diagnostic for the person, never a quota; a targeted ratio gets label-gamed.
2. If every PROCESS item had not happened, what user-visible outcome would differ? "Nothing" means fire the machinery-freeze redirect ([enforcement.md](enforcement.md)) at the offender, yourself first.
3. Honesty sweep, answering each as "No (checked: <what you examined>)" or as a finding with artifact + correction: weakened/skipped tests; gate, tolerance, or suppression-pragma edits riding in feature commits; goldens regenerated to match broken output; claims of unrun commands; fixture/mock output presented as live; softened or buried failures; zero-run greens; closes with unmet acceptance criteria; spec edited to match what was built.
4. Correct every finding on the record, disclose its exact chain to the person in plain words, and tag the record with its pattern ID.

The audit is complete when every item in the window is classified, every honesty check cites evidence, and every finding has all three corrections.

Report verdicts plainly, uncomfortable truths first. An audit finding about your own behavior is something the person must see.
