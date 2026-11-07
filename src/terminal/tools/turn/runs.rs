//! Successful reads, searches, lists, recalls and waits fold as one run after their turn ends.
use super::*;

/// The summary and clocks shared by every call in a folded run.
pub(in crate::terminal) struct Run {
    pub(in crate::terminal) first: EntryId,
    count: usize,
    at: u64,
    // The run's calls' own times added up, shown on its `looked` row.
    ms: u64,
    // How many of each kind of look, and the last one's name.
    routine: [(usize, String); 5],
}

// What a look names when it is the only one of its kind: the file, pattern, place or query.
fn looked(kind: usize, input: &Value) -> String {
    let field = |keys: &[&str]| keys.iter().find_map(|k| input[*k].as_str()).unwrap_or("");
    match kind {
        0 => field(&["path", "file_path"])
            .rsplit('/')
            .next()
            .unwrap_or("")
            .into(),
        1 => field(&["pattern", "query"]).into(),
        2 => field(&["path"]).into(),
        3 => match field(&["query"]) {
            "" => input["id"]
                .as_u64()
                .map_or(String::new(), |id| format!("#{id}")),
            query => format!("“{query}”"),
        },
        _ => String::new(),
    }
}

// The looks that fold: reads, code searches, lists, recalls and waits; a web search does not.
pub(super) fn kind(name: &str, input: &Value) -> Option<usize> {
    match super::super::kind(name) {
        "read" => Some(0),
        "search" => Some(1),
        "list" => Some(2),
        "memory" | "recall" => Some(3),
        "agent" if input["action"] == "wait" => Some(4),
        _ => None,
    }
}

// A run of three or more successful looks folds at `fold`, once its turn has ended.
pub(super) fn locate(run: &mut Vec<&Entry>, places: &mut BTreeMap<EntryId, Place>, fold: u64) {
    if run.len() >= 3 {
        let (mut counts, mut ms): ([(usize, String); 5], u64) = Default::default();
        for entry in run.iter() {
            if let Body::Call { name, input, .. } = &entry.body
                && let Some(i) = kind(name, input)
            {
                counts[i] = (counts[i].0 + 1, looked(i, input));
            }
            let finished = places.get(&entry.id).and_then(|p| p.finished);
            ms += finished.map_or(0, |f| f.saturating_sub(entry.at));
        }
        let shared = Arc::new(Run {
            first: run[0].id,
            count: run.len(),
            at: fold,
            ms,
            routine: counts,
        });
        for (index, entry) in run.iter().enumerate() {
            let place = places.entry(entry.id).or_default();
            place.run = Some(shared.clone());
            place.index = index;
        }
    }
    run.clear();
}

pub(in crate::terminal::tools) fn fold(
    call: &Call<'_>,
    columns: usize,
    now: u64,
    motion: bool,
) -> Option<Vec<Row>> {
    let run = call.place.run.as_ref()?;
    if now < run.at {
        return None;
    }
    let (k, stands, summary) = run.folding(call.place.index, now, motion);
    if !summary && stands {
        // The tick stays on the rail while the row folds away.
        let mut line = row::draw(call, columns, now, motion);
        for part in line.spans.iter_mut().skip(1) {
            part.style.fg = part
                .style
                .fg
                .map(|ink| mix(ink, GLASS, 0.75 * (k / 0.6).min(1.)));
        }
        return Some(vec![Row::plain(line)]);
    }
    if !summary {
        return Some(Vec::new());
    }
    let k = ((k - 0.4) / 0.6).clamp(0., 1.);
    let figure = if columns >= usize::from(PHONE) {
        format!("{} calls", run.count)
    } else {
        String::new()
    };
    let to =
        columns.saturating_sub(if columns >= usize::from(PHONE) { 11 } else { 0 } + width(&figure));
    let text = holds(&run.routine, to.saturating_sub(16));
    let end = 13 + width(&text);
    let mut words = Line::from(vec![
        span("┼", tone(call, now, motion)),
        span("   looked   ", mix(GLASS, GRAY, k)),
        span(text, mix(GLASS, rgb(0xc849ae), k)),
        span(dots(end, to), mix(GLASS, rgb(0x3d464f), k)),
        span(figure, mix(GLASS, GRAY, k)),
    ]);
    if columns >= usize::from(PHONE) {
        let took = dur(run.ms);
        let gap = columns.saturating_sub(3 + width(&took) + words.width());
        let took = format!("{}{took}", " ".repeat(gap));
        words.spans.push(span(took, mix(GLASS, GRAY, k)));
    }
    let action = Some(crate::terminal::transcript::Action::Unfold(call.entry.id));
    Some(vec![Row {
        action,
        ..Row::plain(words)
    }])
}

// The longest of four wordings that fits `room`, the kinds most first: `read theme.rs`, then
// `read 3 files · searched twice`, then `3 read · 2 searches`, then `3 read, 2 search`.
fn holds(routine: &[(usize, String); 5], room: usize) -> String {
    let mut kinds: Vec<_> = (0..5).filter(|&i| routine[i].0 > 0).collect();
    kinds.sort_by_key(|&i| std::cmp::Reverse(routine[i].0));
    let times = |n| match n {
        1 => "once".into(),
        2 => "twice".into(),
        n => format!("{n} times"),
    };
    let phrase = |i: usize, level: usize| {
        let (n, name) = (routine[i].0, &routine[i].1);
        let (one, s) = (n == 1 && level == 0 && !name.is_empty(), plural(n));
        match (i, level) {
            (_, 3) => format!("{n} {}", ["read", "search", "list", "recall", "wait"][i]),
            (0, 2) => format!("{n} read"),
            (1, 2) => format!("{n} search{}", if n == 1 { "" } else { "es" }),
            (2, 2) => format!("{n} listed"),
            (3, 2) => format!("{n} recall{s}"),
            (0, _) if one => format!("read {name}"),
            (0, _) => format!("read {n} file{s}"),
            (1, _) if one => format!("searched {name}"),
            (2, _) if one => format!("listed {name}"),
            (2, _) => format!("listed {n}"),
            (3, _) if one => format!("recalled {name}"),
            (1, _) => format!("searched {}", times(n)),
            (3, _) => format!("recalled {}", times(n)),
            _ => format!("waited {}", times(n)),
        }
    };
    let words = |level| {
        let parts: Vec<_> = kinds.iter().map(|&i| phrase(i, level)).collect();
        parts.join([" · ", ", "][usize::from(level == 3)])
    };
    (0..4)
        .map(words)
        .find(|w| width(w) <= room)
        .unwrap_or_else(|| clip(&words(3), room))
}

impl Run {
    // How far the fold has played, whether the row at `index` still stands, and whether it is the
    // row that becomes the run's `looked` summary.
    fn folding(&self, index: usize, now: u64, motion: bool) -> (f64, bool, bool) {
        let k = played(motion, now, self.at, 750);
        let removed = (self.count as f64 * ease_in_out((k / 0.6).clamp(0., 1.))).round() as usize;
        (k, index >= removed, k > 0.4 && index == 0)
    }
}

pub(super) fn next(call: &Call<'_>, now: u64, motion: bool, mut next: Next) -> Next {
    if let Some(run) = &call.place.run {
        let at = run.at;
        let count = |time| {
            let (_, stands, summary) = run.folding(call.place.index, time, motion);
            usize::from(time < run.at || stands || summary)
        };
        let current = count(now);
        let phases = [0, 300, 301, 450, 750].map(|ms| at.saturating_add(ms));
        if let Some(until) = phases
            .into_iter()
            .filter(|at| *at > now && count(*at) != current)
            .min()
        {
            next.size = next.size.min(change(now, until, count));
        }
        if now >= at && now < at.saturating_add(750) && motion {
            next.look = next.look.min(now.saturating_add(83));
        } else if now >= at.saturating_add(750) && call.place.index > 0 {
            next = Next::NEVER;
        }
    }
    next
}
