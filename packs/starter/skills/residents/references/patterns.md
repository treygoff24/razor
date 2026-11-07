# Worked resident patterns

Choose one pattern, then replace its role, sources and times with the agreed job. Each TOML block is a complete resident entry for the person's settings; the folder and its files must exist first. The Markdown and JSON filenames are conventions you create, not additional Razor settings. All times use the server's local time. For settings mechanics, use `razor`; for every resident field, read the installed `always-on-agents.md`.

## Life chief of staff

Use one home session to accumulate context across triage, a morning brief and an evening review. Its `role.md` sets priorities and authority; `sources.json` names the approved calendar, inbox and commitment sources. Define the three routines with the person: triage identifies changes and deadlines, morning chooses today's priorities, evening reconciles what happened with what remains owed. The resident may organize and draft within its authority; sending mail, booking or spending needs the agreed grant.

```toml
[residents.assistant]
cwd = "~/residents/assistant"
startup = ["~/residents/assistant/role.md", "razor", "about", "project"]
model = "opus"
effort = "high"
night = "03:00"

[residents.assistant.heartbeat]
at = ["10:00", "14:00", "17:00"]
text = "Read routine.md triage and its sources.json entries. Check only changes since the last check. Keep tasks current. Note only an urgent finding within the contact policy; otherwise retain it for the next brief."
model = "haiku"
effort = "low"

[residents.assistant.schedules.morning]
at = ["07:30"]
text = "Read routine.md morning. Review commitments, calendar, inbox and yesterday's unresolved work from approved sources. Write output/morning.md with priorities, deadlines and decisions needed, then send one short note with the main outcome."
model = "opus"
effort = "high"
deliver = "notify"
fresh = false

[residents.assistant.schedules.evening]
at = ["20:00"]
text = "Read routine.md evening. Reconcile today's completed work and open obligations against the evidence. Update tasks and the state note. Write output/evening.md with tomorrow's carryover and send one short note."
model = "opus"
effort = "high"
deliver = "notify"
fresh = false
```

The heartbeat uses the home context even on its cheaper model. Keep its source reads bounded. Leave the two reviews in the home because they need that accumulated context and the evening review writes the state note. Use `/thread assistant` for a garden plan or a trip discussion; close a thread with `/done` so it sends the home what to retain. For continuing subjects with their own files, create resident projects under `projects/`; `always-on-agents.md` describes creating projects and starting threads in them.

## Watcher: inbox, CI, or prices

Use a watcher when a change needs judgment before interrupting someone. Choose one source class first: approved inbox folders and triage rules, repositories and failing CI runs, or public price sources with units and thresholds. In `sources.json`, state exactly what counts as actionable. For price monitoring, preserve the observation time and currency; for CI, the repository, revision and run; for an inbox, the message identifier and reason it needs attention. Read-only access usually suffices.

```toml
[residents.watcher]
cwd = "~/residents/watcher"
startup = ["~/residents/watcher/role.md", "razor", "project"]
model = "haiku"
effort = "low"

[residents.watcher.schedules.check]
at = ["09:00", "12:00", "15:00", "18:00"]
days = ["mon", "tue", "wed", "thu", "fri"]
text = "Read routine.md check and sources.json. Compare approved observations with working/seen.json. Write output/latest.md with findings, source times and unavailable sources. Update the cursor after recording the result. Send one note only for a new actionable change within the contact policy. Leave a concise first line for the home's nightly review."
model = "haiku"
effort = "low"
fresh = true
```

Use a schedule when a missed check should catch up once after downtime; choose a heartbeat instead if a missed check is disposable. `working/seen.json` holds only identifiers and observations needed to distinguish new changes; define its format and first-run behavior in `routine.md`. A missing baseline should produce a baseline, not a flood of historical alerts. The fresh run reads its procedure and cursor each time rather than relying on previous conversation. Leave `deliver` unset so quiet runs send no note. If the job only compares a numeric threshold, use a scheduled script and reserve a resident for interpreting the finding.

## Nightly upkeep

Use this for sorting new material, producing digests and tidying generated work. Separate originals, generated files and proposed changes in the folder instructions. Start with a preview: list intended moves or removals and produce the digest. Add permission for a narrow recurring operation only after its results match the person's intent. Existing delete guards may refuse a deletion; use `razor` and `security.md` to understand them rather than weakening safety to make a routine pass.

```toml
[residents.curator]
cwd = "~/residents/curator"
startup = ["~/residents/curator/role.md", "razor", "project"]
model = "haiku"
effort = "low"
night = "04:00"

[residents.curator.schedules.upkeep]
at = ["02:00"]
text = "Read routine.md upkeep and sources.json. Sort new material within the approved scope, preview any unapproved changes, and clean only generated files explicitly authorized for removal. Preserve originals. Write output/digest.md and output/proposed-changes.json, including failures and unresolved items. Leave a concise final summary for the home session; honor quiet hours."
model = "haiku"
effort = "low"
fresh = true

[residents.curator.schedules.review]
at = ["08:00"]
text = "Read routine.md review, output/digest.md and output/proposed-changes.json. Check their timestamps and the last upkeep run before reporting success. Review failures and proposed changes, update tasks and the state note, then send one useful note with the outcome or blocker."
model = "opus"
effort = "high"
deliver = "notify"
fresh = false
```

The overnight job writes files quietly; its morning review runs in the home to integrate them. A failed scheduled run may still trigger Razor's failure notification. Agree on that behavior before scheduling inside quiet hours. Make upkeep safe to rerun: record which input produced which output and preserve unprocessed items when a source or write fails.

## Data feed for a dashboard or family display

Use a resident when the display needs a human-readable synthesis, such as today's household plan or a summary of several sources. Fixed data fetching and rendering belong in programs. Agree on the audience and exactly which fields may appear on the display; private inbox and calendar details need deliberate selection.

Create `output/feed.json` with a documented schema in `routine.md`, for example `updated_at`, `items` and `unavailable_sources`. Include source timestamps and a stale-data threshold. Publish only a complete valid JSON file, keeping the last good version if construction fails. The display must show when data is stale rather than presenting an old plan as current.

```toml
[residents.display]
cwd = "~/residents/display"
startup = ["~/residents/display/role.md", "razor", "project"]
model = "haiku"
effort = "low"

[residents.display.schedules.refresh]
at = ["06:30", "12:30", "18:30"]
text = "Read routine.md refresh and sources.json. Build output/feed.json from approved sources with update times and unavailable-source markers, following its schema and privacy rules. Parse and check the complete JSON before replacing the last good file. If successful, use deliver with path output/feed.json and a short descriptive line. Leave failures in the record; send a note only when the contact policy requires action."
model = "haiku"
effort = "low"
fresh = false
```

`deliver` snapshots the JSON into the session's files and lists it in the inbox; it is an artifact delivery, so do not add a routine notification as well. Keeping refresh in the home gives the display one stable session id. Read the installed `scripting.md` for authentication and these routes:

- `GET /v1/deliveries?limit=256`: find the newest delivery whose `session` is `display` and whose `file` ends in `/feed.json`.
- `GET /v1/sessions/display/files/<file>`: append that delivery's relative `file` value, preserving its path components. Fetch and parse the snapshot; it is not served directly from the resident's working folder.
- `GET /v1/residents`: inspect the refresh schedule's latest run and error state. `POST /v1/residents/display/run/refresh` requests a new run; it does not return the feed synchronously.

The delivery index is bounded when read, so let the consumer retain the last snapshot it fetched. For a busy home with many other deliveries, use the refresh run's session record to recover its `deliver` result through the entries route in `scripting.md`. `sky` is a separate optional resident setting for daily commitment counts in a phone client; it is not a general JSON endpoint.

Keep Razor's device key in a trusted local service that fetches only the approved feed for the display. A device key grants full Razor access, including starting sessions; putting it in a public webpage or a shared display's browser exposes more than the feed. Remote use follows `scripting.md` and `security.md`; Razor's HTTP server provides no public read-only dashboard token.
