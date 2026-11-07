//! A single layout pass locates calls on their turn's line, including folds and later repairs.
use super::*;
use crate::protocol::{End, EntryId, JobEvent};
use crate::terminal::motion::{Presence, ease_in_out, ease_out, played};
use std::{collections::BTreeMap, sync::Arc};

pub(super) mod runs;

/// Facts from neighbouring mounted entries; never retained beyond the transcript window.
#[derive(Clone, Default)]
pub(in crate::terminal) struct Place {
    pub(in crate::terminal) ended: Option<Ended>,
    /// When this call plays its finish: its end; for one of the newest ten calls that ended
    /// unseen, the person's return plus 80 ms for each replayed call above it; 0, settled, for the
    /// rest of the unseen and while the person is still away. `None` while the call runs.
    pub(in crate::terminal) beat: Option<u64>,
    pub(in crate::terminal) run: Option<Arc<runs::Run>>,
    pub(in crate::terminal) fixed: Option<u64>,
    pub(in crate::terminal) first: bool,
    /// A reply sits between this call and the turn's call before it: the call's rows start with
    /// a blank row, so the line resumes a row below the reply's bar.
    pub(in crate::terminal) after_reply: bool,
    /// The first call each local night (midnight to six) that runs two seconds: the cat bats
    /// along its leader.
    pub(in crate::terminal) cat: bool,
    /// The turn's last call: its closing line draws in under it once the turn ends.
    pub(in crate::terminal) closes: bool,
    /// On an ended turn's last call: one mark per call, in order, for the closing line.
    pub(in crate::terminal) shape: Vec<(&'static str, Color)>,
    /// When the gap above this call turns solid (`gap`); `None` while an unfixed failure lies
    /// above it in the turn.
    pub(in crate::terminal) joined: Option<u64>,
    /// A recorded job end, not the early result that returned its live handle.
    pub(in crate::terminal) finished: Option<u64>,
    pub(in crate::terminal) calls: usize,
    pub(in crate::terminal) failed: usize,
    /// The children its turn launched, its own place among them when it launched one, and how
    /// many launched before it: the threads beside the turn's line.
    pub(in crate::terminal) threads: usize,
    pub(in crate::terminal) tie: Option<usize>,
    pub(in crate::terminal) open: usize,
    asking: bool,
    pub(in crate::terminal) job: Option<JobId>,
    index: usize,
}

/// A turn's end, on each of its calls' places.
#[derive(Clone)]
pub(in crate::terminal) struct Ended {
    /// When its closing line plays: the turn's end; for a turn that ended unseen, the later of the
    /// return and its last call's beat, or 0, settled, when that call settled unseen.
    pub(in crate::terminal) beat: u64,
    /// How it ended, as the closing line names it: `done`, `error` or `stopped`, in its ink.
    pub(in crate::terminal) end: (&'static str, Color),
    /// From the turn's start (its opening input, or the previous turn's end when later) to its
    /// end; absent when the opening input is not mounted.
    pub(in crate::terminal) took: Option<u64>,
    /// Every call of the turn is mounted: a turn end before it is, or its opening input, or the
    /// record's start. Without it the closing line counts nothing from calls.
    pub(in crate::terminal) whole: bool,
    /// What the turn added and wrote: input, cache-write and output tokens; cache reads, the same
    /// context re-read by each request, are left out. Absent when zero.
    pub(in crate::terminal) tokens: Option<u64>,
    /// How many of the turn's failures a later call in it fixed.
    pub(in crate::terminal) fixed: usize,
    /// When the gap above the closing line turns solid, as `Place::joined`.
    pub(in crate::terminal) joined: Option<u64>,
}

/// Each mounted call's place, with one entry pass and bounded passes over those calls.
pub(in crate::terminal) fn places(
    entries: &[&Entry],
    jobs: &BTreeMap<JobId, super::Job>,
    presence: Presence,
    motion: bool,
    zone: Option<&jiff::tz::TimeZone>,
) -> BTreeMap<EntryId, Place> {
    // Every temporary collection is bounded by the mounted entry window (at most 2,000).
    let mut places = BTreeMap::<_, Place>::new();
    let mut ids = BTreeMap::new();
    let mut outcomes = BTreeMap::new();
    let mut turns = Vec::new();
    let mut calls = Vec::new();
    let mut replied = false;
    for entry in entries {
        match &entry.body {
            Body::Call { call, .. } => {
                places.entry(entry.id).or_default().after_reply = std::mem::take(&mut replied);
                ids.insert(call.0.as_str(), entry.id);
                calls.push(*entry);
            }
            Body::Text { .. } | Body::Thinking { .. } => replied = true,
            Body::Result { call, outcome, .. } => {
                if let Some(id) = ids.get(call.0.as_str()) {
                    if let Some(place) = places.get_mut(id) {
                        place.finished = Some(entry.at);
                    }
                    outcomes.insert(*id, *outcome);
                }
            }
            Body::Job(JobEvent::Ended { job, end, .. }) => {
                if let Some(state) = jobs.get(job) {
                    if let Some(place) = places.get_mut(&state.call) {
                        place.finished = Some(entry.at);
                    }
                    outcomes.insert(state.call, job_outcome(*end));
                }
            }
            Body::TurnEnd {
                inputs, end, usage, ..
            } => turns.push((calls.len(), entry.at, inputs, end, usage)),
            _ => {}
        }
    }
    for (job, state) in jobs {
        if let Some(place) = places.get_mut(&state.call) {
            place.job = Some(*job);
        }
        if state.end.is_none() {
            outcomes.remove(&state.call);
            if let Some(place) = places.get_mut(&state.call) {
                place.finished = None;
            }
        } else if let Some(end) = state.end {
            outcomes.insert(state.call, job_outcome(end));
        }
    }
    beats(&calls, &outcomes, &mut places, presence);
    // A running call holds its night's cat; one that ends sooner passes it on at the next layout.
    let mut night = (0, 0, true);
    for e in &calls {
        if e.at >= night.1 {
            let Some((start, end)) = zone.and_then(|z| self::night(z, e.at)) else {
                break;
            };
            night = (start, end, false);
        }
        if e.at >= night.0 && !night.2 {
            let place = places.entry(e.id).or_default();
            let ran = place.finished.unwrap_or(e.at).saturating_sub(e.at);
            place.cat = !outcomes.contains_key(&e.id) || ran >= 2000;
            night.2 = place.cat;
        }
    }
    let (mut start, mut since) = (0, 0);
    let mut bounded = matches!(entries.first().map(|e| &e.body), Some(Body::Start { .. }));
    let mut failures = BTreeMap::new();
    for (end, at, inputs, state, usage) in turns {
        if end > start {
            let last = places.get(&calls[end - 1].id).and_then(|p| p.beat);
            let beat = match presence.missed(at) {
                false => at,
                true if presence.focused => {
                    last.filter(|b| *b > 0).map_or(0, |b| b.max(presence.back))
                }
                true => 0,
            };
            let state = match state {
                End::Done => ("done", DATA),
                End::Error(_) => ("error", AMBER),
                End::Stopped => ("stopped", GRAY),
            };
            let opened = inputs.iter().min().and_then(|id| {
                let i = entries.binary_search_by_key(id, |e| e.id).ok()?;
                Some(entries[i].at.max(since))
            });
            let tokens = usage.input + usage.cache_write + usage.output;
            let ended = Ended {
                beat,
                end: state,
                took: opened.map(|opened| at.saturating_sub(opened)),
                whole: bounded || opened.is_some(),
                tokens: Some(tokens).filter(|n| *n > 0),
                fixed: 0,
                joined: None,
            };
            locate(
                &calls[start..end],
                Some(ended),
                &outcomes,
                &mut places,
                &mut failures,
                motion,
            );
        }
        (start, since, bounded) = (end, at, true);
    }
    let rest = &calls[start..];
    locate(rest, None, &outcomes, &mut places, &mut failures, motion);
    places
}

// The local night (midnight to six) holding `at`, else the next one, in wall ms.
fn night(zone: &jiff::tz::TimeZone, at: u64) -> Option<(u64, u64)> {
    let ms = |z: jiff::Zoned| u64::try_from(z.timestamp().as_millisecond()).ok();
    let here = zoned(at, Some(zone))?;
    let day = match here.hour() < 6 {
        true => here.date(),
        false => here.date().tomorrow().ok()?,
    };
    let six = day.at(6, 0, 0, 0).to_zoned(zone.clone()).ok()?;
    Some((ms(day.to_zoned(zone.clone()).ok()?)?, ms(six)?))
}

// Each decided call's beat: the newest ten that ended unseen replay from the person's return, 80
// ms apart, top to bottom; the older unseen, and all of them while the person is away, settle.
fn beats(
    calls: &[&Entry],
    outcomes: &BTreeMap<EntryId, Outcome>,
    places: &mut BTreeMap<EntryId, Place>,
    presence: Presence,
) {
    let end = |e: &Entry, p: &Place| outcomes.get(&e.id).map(|_| p.finished.unwrap_or(e.at));
    let unseen = calls.iter().filter_map(|e| end(e, places.get(&e.id)?));
    let unseen = unseen.filter(|at| presence.missed(*at)).count();
    let (first, mut i) = (unseen.saturating_sub(10), 0);
    for e in calls {
        let place = places.entry(e.id).or_default();
        place.beat = end(e, place).map(|at| match presence.missed(at) {
            false => at,
            true => {
                i += 1;
                match presence.focused && i > first {
                    true => presence.back + 80 * (i - 1 - first) as u64,
                    false => 0,
                }
            }
        });
    }
}

/// Whether `entry` launches a child: the agent tool's start.
pub(in crate::terminal) fn launch(entry: &Entry) -> bool {
    matches!(&entry.body,
        Body::Call { name, input, .. } if kind(name) == "agent" && input["action"] == "start")
}

/// The turn's line and its children's threads: two threads beside it at most on the phone and
/// four wider, later children sharing the last.
pub(in crate::terminal) fn slots(threads: usize, columns: usize) -> usize {
    (threads + 1).min(if columns < usize::from(PHONE) { 3 } else { 5 })
}

/// Splices the turn's child threads in right of a row's first cell, `open` of them lit: a launch
/// ties its thread on at slot `tie` (`├─┼─╮`), the closing rule (`end`) runs across them, a
/// card's edge leaving the line crosses the lit ones, and any other row passes them by.
pub(super) fn thread(
    line: &mut Line<'static>,
    slots: usize,
    open: usize,
    tie: Option<usize>,
    end: bool,
) {
    // The edge leaving the line, by the cell after the first.
    let mut cells = line
        .spans
        .iter()
        .flat_map(|s| s.content.chars().map(|c| (c, s.style)));
    let edge = cells.nth(1).filter(|(c, _)| *c == '─').map(|(_, ink)| ink);
    let Some(first) = line.spans.first_mut() else {
        return;
    };
    let Some(cell) = first.content.chars().next() else {
        return;
    };
    let (own, rest) = (first.style, first.content[cell.len_utf8()..].to_owned());
    first.content = String::from(if tie.is_some() { '├' } else { cell }).into();
    let live = style(rgb(0x22773c), GLASS);
    let mut cells = Vec::with_capacity(2 * slots);
    for k in 1..slots {
        let lit = k <= open;
        let (conn, glyph, ink) = match (tie, edge) {
            (Some(to), _) if k == to => ("─", "╮", live),
            (Some(to), _) if k < to => ("─", if lit { "┼" } else { "─" }, live),
            (_, Some(edge)) => ("─", if lit && !end { "┼" } else { "─" }, edge),
            _ => (" ", if lit { "│" } else { " " }, live),
        };
        cells.extend([Span::styled(conn, ink), Span::styled(glyph, ink)]);
    }
    cells.push(Span::styled(rest, own));
    line.spans.splice(1..1, cells);
}

fn locate<'a>(
    calls: &[&'a Entry],
    end: Option<Ended>,
    outcomes: &BTreeMap<EntryId, Outcome>,
    places: &mut BTreeMap<EntryId, Place>,
    failures: &mut BTreeMap<(&'a str, &'a str), Vec<EntryId>>,
    motion: bool,
) {
    let failed = calls
        .iter()
        .filter(|e| outcomes.get(&e.id).is_some_and(|o| *o != Outcome::Success))
        .count();
    let live = calls.iter().any(|e| !outcomes.contains_key(&e.id));
    // Runs of looks fold 1,300 ms after the turn's closing line plays; with motion off, as it does.
    let fold = end
        .as_ref()
        .map_or(u64::MAX, |e| e.beat + if motion { 1300 } else { 0 });
    let asking = calls.iter().any(|e| !outcomes.contains_key(&e.id) && matches!(&e.body,
        Body::Call { name, input, .. } if kind(name) == "ask" || kind(name) == "message" && input["type"] == "question"));
    let ties = |e: &Entry| launch(e) && outcomes.get(&e.id).is_none_or(|o| *o == Outcome::Success);
    let threads = calls.iter().filter(|e| ties(e)).count();
    let (mut routine, mut open) = (Vec::new(), 0);
    // This turn's failures by position, each with its fixing call's position and fix's instant.
    let mut flaws: Vec<(usize, Option<(usize, u64)>)> = Vec::new();
    for (i, entry) in calls.iter().enumerate() {
        let place = places.entry(entry.id).or_default();
        (place.threads, place.open) = (threads, open);
        place.tie = ties(entry).then_some(open);
        place.first = i == 0;
        place.closes = i + 1 == calls.len();
        place.calls = calls.len();
        place.failed = failed;
        place.asking = asking;
        open += usize::from(ties(entry));
        let Body::Call { name, input, .. } = &entry.body else {
            continue;
        };
        let target = ["path", "file_path", "command", "name", "query"]
            .into_iter()
            .find_map(|k| input[k].as_str())
            .unwrap_or("");
        let key = (kind(name), target);
        match outcomes.get(&entry.id) {
            Some(Outcome::Success) => {
                let at = places.get(&entry.id).and_then(|p| p.beat);
                let at = at.map(|at| at.saturating_add(900));
                for id in failures.remove(&key).into_iter().flatten() {
                    places.entry(id).or_default().fixed = at;
                    let flaw = flaws.iter_mut().find(|(f, _)| calls[*f].id == id);
                    if let (Some((_, fix)), Some(at)) = (flaw, at) {
                        *fix = Some((i, at));
                    }
                }
            }
            Some(_) => {
                failures.entry(key).or_default().push(entry.id);
                flaws.push((i, None));
            }
            _ => {}
        }
        if runs::kind(name, input).is_some() && outcomes.get(&entry.id) == Some(&Outcome::Success) {
            routine.push(*entry);
        } else {
            runs::locate(&mut routine, places, fold);
        }
    }
    runs::locate(&mut routine, places, fold);
    for (j, entry) in calls.iter().enumerate() {
        places.entry(entry.id).or_default().joined = joined(&flaws, j);
    }
    let Some(mut ended) = end.filter(|_| !live) else {
        return;
    };
    ended.joined = joined(&flaws, calls.len());
    let fixed = |e: &&&Entry| places.get(&e.id).is_some_and(|p| p.fixed.is_some());
    ended.fixed = calls.iter().filter(fixed).count();
    for entry in calls {
        places.entry(entry.id).or_default().ended = Some(ended.clone());
    }
    if let Some(last) = calls.last()
        && ended.whole
    {
        let place = places.entry(last.id).or_default();
        place.shape = calls.iter().map(|e| mark(e, outcomes.get(&e.id))).collect();
    }
}

// When the gap above the `j`-th call turns solid: at once with no failure above it in the turn,
// never while one is unfixed, else as the spark from each reaches it, running from the fix down
// to its fixing call in 500 ms and on at that pace.
fn joined(flaws: &[(usize, Option<(usize, u64)>)], j: usize) -> Option<u64> {
    let mut above = flaws.iter().filter(|(i, _)| *i < j);
    above.try_fold(0, |at, (i, fix)| {
        let (f, fixed) = (*fix)?;
        Some(at.max(fixed + 500 * (j - i) as u64 / (f - i) as u64))
    })
}

/// The gap above a call: dashed amber until joined, the spark for 100 ms as it joins, then solid
/// in the turn's tone. Motion off joins with no spark.
pub(super) fn gap(joined: Option<u64>, call: &Call<'_>, now: u64, motion: bool) -> Span<'static> {
    match joined {
        Some(at) if now >= at.saturating_add(100) || !motion && now >= at => {
            span("│", tone(call, now, motion))
        }
        Some(at) if now >= at => span("✦", mix(AMBER, WHITE, 0.5)),
        _ => span("┆", rgb(0xc18816)),
    }
}

// A call's mark on its turn's closing line, in its kind's colour; a failure's in amber.
fn mark(entry: &Entry, outcome: Option<&Outcome>) -> (&'static str, Color) {
    let (name, input) = match &entry.body {
        Body::Call { name, input, .. } => (shown(name, input), input),
        _ => ("", &Value::Null),
    };
    match (outcome, name) {
        (Some(Outcome::Error), _) => ("✗", AMBER),
        (Some(Outcome::Unknown), _) => ("?", RED),
        (_, "read" | "search" | "web" | "list") => ("▫", GRAY),
        (_, "edit" | "write" | "apply_patch") => ("▪", rgb(0xda4fbd)),
        (_, "shell") => ("▪", rgb(0x35c45e)),
        (_, "agent") => ("▪", rgb(0xd6a0ff)),
        (_, "ask") => ("▪", rgb(0x2b90a1)),
        (_, "message") if input["type"] == "question" => ("▪", rgb(0x2b90a1)),
        _ => ("▪", GRAY),
    }
}

pub(super) fn tone(call: &Call<'_>, now: u64, motion: bool) -> Color {
    if call.place.asking {
        return rgb(0x2b90a1);
    }
    let ended = call.place.ended.as_ref();
    let k = ended.map_or(0., |e| played(motion, now, e.beat, 2260));
    mix(rgb(0x22773c), rgb(0x41494f), k)
}

/// The closing line under an ended turn's last call: its shape, then `done · 9 calls · ~41k
/// tokens · 2m 14s` drawn in from its beat, the time counting up. The phone's compact form drops
/// a plain `done`, the failures (the shape shows them) and the word `tokens`. The marks take the
/// cells left; past that, each stands for several calls and shows a failure among them.
pub(super) fn footer(call: &Call<'_>, columns: usize, now: u64, motion: bool) -> Vec<Row> {
    let Some(ended) = call.place.ended.as_ref().filter(|_| call.place.closes) else {
        return Vec::new();
    };
    let k = played(motion, now, ended.beat.saturating_add(1660), 600);
    if k == 0. {
        return Vec::new();
    }
    let mut words = said(call, ended, columns >= usize::from(PHONE));
    let used = |words: &[Span<'_>]| words.iter().map(Span::width).sum::<usize>();
    if used(&words) + 5 > columns {
        words = said(call, ended, false);
    }
    let shape = &call.place.shape;
    let group = shape
        .len()
        .div_ceil(columns.saturating_sub(4 + used(&words)).max(1));
    let marks = shape.chunks(group.max(1)).map(|calls| {
        let failed = calls.iter().find(|(glyph, _)| matches!(*glyph, "✗" | "?"));
        *failed.unwrap_or(&calls[0])
    });
    let fade = ((k - 0.45) / 0.55).clamp(0., 1.);
    let shown = shape.len().div_ceil(group.max(1)) as f64 * ((k - 0.15) / 0.4).clamp(0., 1.);
    let mut spans = vec![span(if k < 0.1 { "╰  " } else { "╰─ " }, rgb(0x41494f))];
    spans.extend(
        marks
            .enumerate()
            .map(|(i, (glyph, ink))| match (i as f64) < shown.round() {
                true => span(glyph, ink),
                false => Span::raw(" "),
            }),
    );
    spans.push(Span::raw(" "));
    if let (Some(ms), Some(time)) = (ended.took.filter(|_| k < 1.), words.last_mut()) {
        time.content = took((ms as f64 * ease_out(fade)) as u64).into();
    }
    for (i, mut word) in words.into_iter().enumerate() {
        if i > 0 {
            spans.push(span(" · ", mix(GLASS, GRAY_DIM, fade)));
        }
        word.style.fg = word.style.fg.map(|ink| mix(GLASS, ink, fade));
        spans.push(word);
    }
    vec![
        Row::plain(Line::from(gap(ended.joined, call, now, motion))),
        Row::plain(Line::from(spans)),
    ]
}

// The closing line's words, the time last: wide, `done · 9 calls · 1 failed, then fixed · ~41k
// tokens · 2m 14s`; compact, `9 calls · ~41k · 2m 14s`, naming only an end that is not done. A
// turn not all mounted counts nothing from its calls.
fn said(call: &Call<'_>, ended: &Ended, wide: bool) -> Vec<Span<'static>> {
    let ((label, ink), n, failed) = (ended.end, call.place.calls, call.place.failed);
    let quiet = |words: String| span(words, GRAY);
    let mut words = Vec::with_capacity(5);
    if wide || label != "done" {
        words.push(Span::styled(label, bold(ink, GLASS)));
    }
    if ended.whole {
        words.push(quiet(format!("{n} call{}", plural(n))));
        if wide && failed > 0 {
            words.push(match ended.fixed == failed {
                true => span(
                    format!("{failed} failed, then fixed"),
                    mix(AMBER, GRAY, 0.45),
                ),
                false => span(format!("{failed} failed"), AMBER),
            });
        }
    }
    if let Some(tokens) = ended.tokens.map(kilo) {
        words.push(quiet(match wide {
            true => format!("~{tokens} tokens"),
            false => format!("~{tokens}"),
        }));
    }
    words.extend(ended.took.map(|ms| quiet(took(ms))));
    words
}

// A turn's length: `14s`, `2m 14s`, `1h 02m`.
fn took(ms: u64) -> String {
    match ms / 1000 {
        s @ ..60 => format!("{s}s"),
        s @ ..3600 => format!("{}m {:02}s", s / 60, s % 60),
        s => format!("{}h {:02}m", s / 3600, s / 60 % 60),
    }
}

pub(super) fn next(call: &Call<'_>, now: u64, motion: bool, mut next: Next) -> Next {
    next = runs::next(call, now, motion, next);
    // A spark reaching the gap above, or the closing line's.
    let below = call.place.ended.as_ref().filter(|_| call.place.closes);
    let below = below.and_then(|e| e.joined);
    let joins = [call.place.joined, below].into_iter().flatten();
    let joins = joins.flat_map(|at| [at, at.saturating_add(100)]);
    next.look = joins.filter(|at| *at > now).fold(next.look, u64::min);
    if let Some(Ended { beat: at, .. }) = &call.place.ended {
        let end = at.saturating_add(2260);
        if motion && now < end {
            next.look = next.look.min(now.saturating_add(83).max(*at));
        }
        if motion && call.place.closes && now <= at.saturating_add(1660) {
            next.size = next.size.min(at.saturating_add(1661));
        }
    }
    next.look = next.look.min(next.size);
    next
}
