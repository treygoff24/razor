//! Razor's own memory (docs/design/memory.md): one Markdown file per memory in the server's
//! home, in `memory/all/` for every project and in `memory/projects/<the project's path>/` for
//! one, and a retired memory at the same place under `memory/retired/`. A file's name says its
//! kind, state notes are `state/<session>.md` with their merged note at `state.md`, and a header between `---` lines holds its
//! source, any expiry and a fact's or note's description. A resident's own layer is the same
//! layout in `residents/<name>/` inside its project's folder. Nothing is held between calls, and
//! everything here runs on a blocking thread. Task files share the reader, header and writer.

use super::{Answer, Error, Kind, TEXT, Write, citations, fill, other, refuse};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::collections::hash_map::RandomState;
use std::fs::{self, File};
use std::hash::{BuildHasher, Hasher};
use std::io::{ErrorKind, Read, Write as _};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Component, Path, PathBuf};
use std::time::SystemTime;

/// The most of one file a read holds: a written memory's 16 KiB of text and its header.
pub(super) const FILE: u64 = 17 * 1024;
/// The most memories one read keeps: the page's newest rulings, or the person's list.
pub(super) const KEEP: usize = 64;
/// The startup page's rulings, and its list of what is known: at most this many lines, in this
/// many bytes (about 1,000 and 600 tokens).
const RULINGS: (usize, usize) = (20, 4000);
const KNOWN: (usize, usize) = (24, 2400);
/// The glossary's lines on the page, taken out of the list of what is known.
const GLOSSARY: (usize, usize) = (12, 1200);

/// The folders of one project's memories and of every project's, each pair in that order, and who
/// added the project's rulings when it is shared.
#[derive(Clone)]
pub(super) struct Folders {
    pub(super) current: Vec<PathBuf>,
    retired: [PathBuf; 2],
    pub(super) owners: Owners,
}

/// Who added each ruling of a shared project, by its id: their name, and whether they are this
/// home's person. Empty for a project that is not shared.
pub(super) type Owners = HashMap<String, (String, bool)>;

impl Folders {
    /// These folders with `resident`'s own layer, inside the project's, in place of the project's.
    pub(super) fn layer(&self, resident: Option<&str>) -> Folders {
        let mut layer = self.clone();
        for folder in [&mut layer.current[0], &mut layer.retired[0]] {
            folder.extend(resident.iter().flat_map(|&name| ["residents", name]));
        }
        layer
    }
}

/// One memory or task file as a read found it: at most `FILE` bytes, and when it was written, by
/// its id's stamp, else when the file last changed.
pub(super) struct Found {
    pub(super) id: String,
    pub(super) at: SystemTime,
    pub(super) file: String,
}

/// The memory folders of the project kept for `root` and of every project: its path's
/// components under `projects/`.
pub(super) fn folders(dir: &Path, root: &Path) -> Folders {
    let normal = |c: &Component| matches!(c, Component::Normal(_));
    let project = root.components().filter(normal).collect::<PathBuf>();
    let both = |base: PathBuf| [base.join("projects").join(&project), base.join("all")];
    let (current, retired) = (Vec::from(both(dir.into())), both(dir.join("retired")));
    Folders {
        current,
        retired,
        owners: Owners::new(),
    }
}

/// The page's memory part under `heads`: the state note; the standing rulings, newest first, each
/// marked as set aside when a standing ruling settles it; and the descriptions of facts and notes,
/// newest first, then how many more there are. With it, a question for the person for each of
/// their rulings that another person's agreement set aside.
pub(super) fn page(folders: &Folders, heads: &[String; 3]) -> Result<(String, Vec<String>), Error> {
    let standing = |f: &Found| header(&f.file, "expires").is_none().then_some(0);
    let rulings = best(folders, KEEP, |id| kind(id) == "ruling", standing)?.0;
    let mut asks = Vec::new();
    let lines = rulings.iter().map(|f| {
        let settles = |by: &&Found| head(&by.file).any(|(k, v)| k == "settles" && v == f.id);
        let by = rulings.iter().find(settles).map(|by| &by.id);
        if let Some(by) = by
            && matches!(folders.owners.get(&f.id), Some((_, true)))
        {
            asks.push(fill(&TEXT.restore, &[("id", &f.id), ("by", by)]));
        }
        let aside = by.map(|by| TEXT.set_aside.replace("{by}", by));
        ruling_line(folders, f) + &aside.unwrap_or_default()
    });
    let differ = (!folders.owners.is_empty()).then_some(TEXT.differ.as_str());
    let rulings = fit(lines, RULINGS).0 + differ.unwrap_or_default();
    let terms = glossary(folders);
    let room = (KNOWN.0 - terms.lines().count(), KNOWN.1 - terms.len());
    let parts = [state(folders), rulings, terms + &known(folders, room)?];
    let empty = [&TEXT.no_state, &TEXT.none, &TEXT.none];
    let page = (heads.iter().zip(parts).zip(empty)).map(|((head, part), empty)| {
        format!("{head}\n\n{}", if part.is_empty() { empty } else { &part })
    });
    Ok((page.collect::<Vec<_>>().join("\n\n"), asks))
}

/// The night's index includes every current ruling, including old and expiring ones.
pub(super) fn rulings(folders: &Folders) -> Result<String, Error> {
    let most = (200, 16 << 10);
    let (found, more) = best(folders, most.0, |id| kind(id) == "ruling", |_| Some(0))?;
    let (mut lines, left) = fit(found.iter().map(|f| ruling_line(folders, f)), most);
    if more + left > 0 {
        lines += &format!("\n{}", TEXT.more.replace("{n}", &(more + left).to_string()));
    }
    Ok(lines)
}

/// A ruling's line: its rule, the name of whoever added it to a shared project, and its id.
fn ruling_line(folders: &Folders, f: &Found) -> String {
    let (said, owner) = (rule(split(&f.file).1), folders.owners.get(&f.id));
    let owner = owner.map(|(name, mine)| TEXT.owned[usize::from(*mine)].replace("{name}", name));
    let owner = owner.unwrap_or_default();
    format!("- {said}{owner} ({})", f.id)
}

/// The current state note, if present and unexpired, else nothing.
fn state(folders: &Folders) -> String {
    let merged = read(&folders.current[0].join("state.md"), "state").ok();
    let since = merged
        .as_ref()
        .map(|f| f.at)
        .unwrap_or(SystemTime::UNIX_EPOCH);
    let pick = |id: &str| id.starts_with("state/");
    let notes = best(folders, KEEP, pick, |f| (f.at > since).then_some(0));
    let notes = notes.map(|(notes, _)| notes).unwrap_or_default();
    let today = jiff::Zoned::now().date().to_string();
    let merged = merged.filter(|f| !expired(&f.file, &today));
    let bodies = merged.iter().chain(&notes).map(|f| split(&f.file).1);
    let lines = bodies.flat_map(str::lines).map(str::to_owned);
    fit(lines, (usize::MAX, super::STATE + 1)).0
}

/// The terms every session starts with: the lines of the wiki page `glossary` that fit, under a
/// lead line that says how many more `memory get` reads; nothing for no page or an empty one.
fn glossary(folders: &Folders) -> String {
    let page = read(
        &folders.current[0].join("wiki/glossary.md"),
        "wiki/glossary",
    );
    let page = page.map(|f| f.file).unwrap_or_default();
    let lines = split(&page).1.lines().filter(|l| !l.trim().is_empty());
    let (lines, more) = fit(lines.map(str::to_owned), GLOSSARY);
    if lines.is_empty() {
        return lines;
    }
    let lead = TEXT.glossary[usize::from(more > 0)].replace("{n}", &more.to_string());
    format!("{lead}\n{lines}\n")
}

/// The descriptions of facts and notes, newest first, in at most `most` lines and bytes, then a
/// line counting the rest: the page's list, and the night's when longer.
pub(super) fn known(folders: &Folders, most: (usize, usize)) -> Result<String, Error> {
    let since = fs::read_to_string(folders.current[0].join(".dream"))
        .ok()
        .and_then(|s| s.lines().next().and_then(|s| s.parse::<u64>().ok()))
        .map(|ms| SystemTime::UNIX_EPOCH + std::time::Duration::from_millis(ms))
        .unwrap_or(SystemTime::UNIX_EPOCH);
    // The page shows the glossary's own lines, so it lists that page only for the night.
    let pick = |id: &str| {
        id != "wiki/_instructions"
            && (id != "wiki/glossary" || most.0 > KNOWN.0)
            && matches!(kind(id), "fact" | "note" | "page")
    };
    let rank = |f: &Found| {
        let page = kind(&f.id) == "page";
        (page || f.at > since || most.0 > KNOWN.0).then_some(usize::from(page))
    };
    let (found, more) = best(folders, most.0, pick, rank)?;
    let line = |f: &Found| {
        let first = split(&f.file).1.lines().find(|l| !l.trim().is_empty());
        let first = first.map_or("", |l| &l[..l.floor_char_boundary(200)]);
        let said = header(&f.file, "description").unwrap_or(first);
        format!("- {said} ({})", f.id)
    };
    let (mut lines, left) = fit(found.iter().map(line), most);
    if more + left > 0 {
        lines += &format!("\n{}", TEXT.more.replace("{n}", &(more + left).to_string()));
    }
    Ok(lines)
}

/// The memories whose text holds the most of `query`'s words, matched whole in lowercase,
/// newest first among equals; every memory when it has no words.
pub(super) fn search(folders: &Folders, query: &str, limit: usize) -> Answer {
    let rank = |f: &Found| {
        let (held, wanted) = held(split(&f.file).1, query);
        (held > 0 || wanted == 0).then_some(held)
    };
    let (found, more) = best(folders, limit, |_| true, rank)?;
    Ok(hits(&found, more))
}

/// How many of `query`'s words `text` holds, matched whole in lowercase, and how many it has.
pub(super) fn held(text: &str, query: &str) -> (usize, usize) {
    let (text, query) = (text.to_lowercase(), query.to_lowercase());
    let wanted: Vec<&str> = query.split(apart).filter(|w| !w.is_empty()).collect();
    let holds = |w: &&&str| text.split(apart).any(|t| t == **w);
    (wanted.iter().filter(holds).count(), wanted.len())
}

/// Memory `id`: its kind, and its file as it is written, header and all.
pub(super) fn get(folders: &Folders, id: &str) -> Answer {
    let (_, _, found) = find(folders, id)?;
    Ok(json!({"id": id, "kind": kind(id), "file": found.file}))
}

/// Writes `w` as a new file, a session or merged state note, or a wiki page. The complete file is
/// staged and synced before its predecessor retires; of competing replacements, only the one
/// that retires the old file publishes.
pub(super) fn write(folders: &Folders, kind: Kind, mut w: Write) -> Answer {
    let mut previous = Vec::new();
    for old in w.replaces.as_ref().map_or(&[][..], |r| r.ids()) {
        let (path, _, found) = find(folders, old)?;
        if kind == Kind::Ruling && fs::metadata(&path).is_ok_and(|m| m.len() > FILE) {
            return Err(refuse(&TEXT.file_long));
        }
        let expected = &folders.current[usize::from(w.all)];
        if !path.starts_with(expected) {
            return Err(refuse(&TEXT.scope));
        }
        previous.push(found);
    }
    for id in &w.settles {
        find(folders, id)?;
    }
    if kind == Kind::Ruling {
        ruling(&mut w, &previous)?;
    }
    let tail = stamp();
    let id = match kind {
        Kind::Ruling => format!("ruling-{tail}"),
        Kind::Fact => format!("fact-{tail}"),
        Kind::Note => format!("note-{tail}"),
        Kind::State if w.dreaming => "state".into(),
        Kind::State => format!("state/{}", w.session),
        Kind::Page => format!("wiki/{}", w.name.as_deref().unwrap_or_default()),
    };
    let folder = &folders.current[usize::from(w.all)];
    if kind == Kind::Page && !folder.join(format!("{id}.md")).exists() {
        let count = fs::read_dir(folder.join("wiki"))
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| {
                e.file_name() != "_instructions.md"
                    && e.path().extension().is_some_and(|x| x == "md")
            })
            .count();
        let replacing_page = w
            .replaces
            .as_ref()
            .is_some_and(|r| r.ids().iter().any(|id| id.starts_with("wiki/")));
        if count >= 200 + usize::from(replacing_page) {
            return Err(refuse(&TEXT.pages_full));
        }
    }
    let line =
        |key, values: &[String]| String::from_iter(values.iter().map(|v| format!("{key}: {v}\n")));
    let tail = line("expires", w.expires.as_slice()) + &line("name", w.name.as_slice());
    let tail = tail + &line("checked", w.checked.as_slice());
    let tail = tail + &line("description", w.description.as_slice()) + &line("cites", &w.cites);
    let tail = tail + &line("settles", &w.settles);
    let file = format!("---\nsource: {}\n{tail}---\n{}", w.source, w.text);
    if file.len() as u64 > FILE {
        return Err(refuse(&TEXT.file_long));
    }
    if secret(&file) {
        return Err(refuse(&TEXT.secret));
    }
    let path = folder.join(format!("{id}.md"));
    let temp = stage(path.parent().unwrap_or(folder), &file)?;
    if let Some(Err(e)) = w
        .replaces
        .as_ref()
        .map(|old| retire(folders, old.ids(), false))
    {
        let _ = fs::remove_file(&temp);
        return Err(e);
    }
    fs::rename(temp, path).map_err(other("publishing a memory file"))?;
    Ok(json!({"id": id, "replaced": w.replaces}))
}

/// Keeps every original citation and exact quotation when a rule replaces other rules.
fn ruling(w: &mut Write, previous: &[Found]) -> Result<(), Error> {
    let mut cites = Vec::new();
    for found in previous {
        let body = split(&found.file).1;
        // Older files embedded the verified words before the agent's reading; this reader and
        // `rule`'s fallback go once the one-time migration has retired every old-form ruling.
        let old = body.rsplit_once("\"\n\nagent's reading:");
        let legacy = old.and_then(|(said, _)| {
            said.strip_prefix("the human said (")
                .or_else(|| said.split_once("\"\n\nthe human said (").map(|(_, s)| s))
                .and_then(|s| s.split_once("): \""))
                .map(|(source, words)| (source, words.to_owned()))
        });
        let blocks = body.split("\n\n").skip(1).take_while(|_| old.is_none());
        let current = blocks.filter_map(|block| {
            let (source, words) = block.split_once('\n')?;
            let lines = words.split('\n').map(|line| line.strip_prefix("> "));
            Some((source, lines.collect::<Option<Vec<_>>>()?.join("\n")))
        });
        // A carried quotation keeps the source it was recorded under, a pre-Razor one included;
        // only a new one must be the person's entry, which `cite` has checked.
        let before = w.quotes.len();
        let carried = legacy.into_iter().chain(current);
        w.quotes
            .extend(carried.map(|(source, words)| (source.to_owned(), words)));
        if before == w.quotes.len() {
            return Err(refuse(&TEXT.ruling_sources));
        }
        let carried = |key| head(&found.file).filter(move |(k, _)| *k == key);
        cites.extend(carried("cites").map(|(_, v)| v.to_owned()));
        w.settles
            .extend(carried("settles").map(|(_, v)| v.to_owned()));
    }
    w.quotes.sort();
    w.quotes.dedup();
    let sources = w.quotes.iter().map(|(source, _)| source.clone());
    cites.extend(sources.filter(|source| citations::valid(source)));
    if w.cites.iter().any(|c| !cites.contains(c)) {
        return Err(refuse(&TEXT.ruling_sources));
    }
    cites.sort();
    cites.dedup();
    if cites.len() > 32 {
        return Err(refuse(&TEXT.cites));
    }
    w.cites = cites;
    for (source, words) in &w.quotes {
        let quote = words
            .split('\n')
            .map(|line| format!("> {line}"))
            .collect::<Vec<_>>()
            .join("\n");
        w.text += &format!("\n\n{source}\n{quote}");
    }
    if w.text.len() > 16 * 1024 {
        return Err(refuse(&TEXT.too_long));
    }
    Ok(())
}

/// Moves memories `ids`, all in one scope, into the retired tree, where search, get and the page
/// no longer see them; with `forget`, the person's word, deletes them instead, words and all.
/// Every id is found, and its scope and a shared ruling's owner checked, before any file moves.
/// Each move is one rename or removal, so of calls that retire one memory at once, one succeeds
/// and the others are refused.
pub(super) fn retire(folders: &Folders, ids: &[String], forget: bool) -> Answer {
    let mut found = Vec::new();
    for id in ids {
        if id == "wiki/_instructions" {
            return Err(refuse(&TEXT.instructions));
        }
        if let Some((name, false)) = folders.owners.get(id) {
            return Err(refuse(&TEXT.owner.replace("{name}", name)));
        }
        let (path, to, _) = find(folders, id)?;
        let first = found
            .first()
            .map(|(p, _): &(PathBuf, PathBuf)| p.starts_with(&folders.current[1]));
        if first.is_some_and(|all| all != path.starts_with(&folders.current[1])) {
            return Err(refuse(&TEXT.scope));
        }
        found.push((path, to));
    }
    for (path, to) in found {
        let done = match forget {
            true => fs::remove_file(&path),
            false => {
                fs::create_dir_all(to.parent().unwrap_or(&to)).and_then(|_| fs::rename(&path, &to))
            }
        };
        done.map_err(|e| match e.kind() {
            ErrorKind::NotFound => refuse(&TEXT.gone),
            _ => other("retiring a memory file")(e),
        })?;
    }
    Ok(json!({ "retired": ids }))
}

/// Writes `text` as `name` in `folder`, making it: a temporary dotfile, `sync_all`, then one
/// rename, so a reader or a crash sees the old file or the new one, never half of one.
pub(super) fn put(folder: &Path, name: &str, text: &str) -> Result<(), Error> {
    fs::rename(stage(folder, text)?, folder.join(name))
        .map_err(other("publishing a memory or task file"))
}

/// A complete, synced dotfile in `folder`, ready for publication by one rename.
fn stage(folder: &Path, text: &str) -> Result<PathBuf, Error> {
    let temp = folder.join(format!(".{}.tmp", stamp()));
    let put = || {
        fs::create_dir_all(folder)?;
        let mut out = File::create_new(&temp)?;
        out.write_all(text.as_bytes())?;
        out.sync_all()
    };
    put().map_err(other("writing a memory or task file"))?;
    Ok(temp)
}

/// The time to the second and eight random hex digits: a new file's name, unique in practice.
pub(super) fn stamp() -> String {
    let random = RandomState::new().build_hasher().finish() as u32;
    let time = jiff::Timestamp::now().strftime("%Y%m%d-%H%M%S");
    format!("{time}-{random:08x}")
}

/// The `keep` memories in `folders` whose ids `pick` takes and that `rank` ranks, higher first
/// and then newest, read one file at a time; and how many more it ranked. A memory past its
/// `expires` date is moved into the retired tree as the read passes it.
pub(super) fn best(
    folders: &Folders,
    keep: usize,
    pick: impl Fn(&str) -> bool,
    rank: impl Fn(&Found) -> Option<usize>,
) -> Result<(Vec<Found>, usize), Error> {
    let today = jiff::Zoned::now().date().to_string();
    let (mut kept, mut ranked) = (Vec::<(usize, Found)>::new(), 0);
    for (folder, retired) in folders.current.iter().zip(&folders.retired) {
        let entries = match fs::read_dir(folder) {
            Err(e) if e.kind() == ErrorKind::NotFound => continue,
            entries => entries.map_err(other("reading memory files"))?,
        };
        let nested = ["state", "wiki"].into_iter().flat_map(|sub| {
            fs::read_dir(folder.join(sub))
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| e.path())
        });
        let paths = entries.flatten().map(|e| e.path()).chain(nested);
        for path in paths {
            let name = path
                .strip_prefix(folder)
                .unwrap_or(&path)
                .to_string_lossy()
                .into_owned();
            let id = name.strip_suffix(".md").filter(|id| pick(id));
            // A folder, a pipe, or a file removed since the listing is no memory.
            let Some(found) = id.and_then(|id| read(&path, id).ok()) else {
                continue;
            };
            if expired(&found.file, &today) {
                let to = retired.join(&name);
                let moved = fs::create_dir_all(to.parent().unwrap_or(retired))
                    .and_then(|_| fs::rename(&path, &to));
                moved.unwrap_or_else(|e| eprintln!("razor: retiring expired memory {name}: {e}"));
                continue;
            }
            let Some(rank) = rank(&found) else {
                continue;
            };
            ranked += 1;
            let at = kept.partition_point(|(r, f)| (*r, f.at, &f.id) > (rank, found.at, &found.id));
            kept.insert(at, (rank, found));
            kept.truncate(keep);
        }
    }
    let more = ranked - kept.len();
    Ok((kept.into_iter().map(|(_, found)| found).collect(), more))
}

/// The lines that fit, in order: at most `most.0` of them in `most.1` bytes, passing over a line
/// that does not fit rather than cutting it; and how many were passed over.
pub(super) fn fit(lines: impl Iterator<Item = String>, most: (usize, usize)) -> (String, usize) {
    let (mut kept, mut room, mut left) = (Vec::new(), most.1, 0);
    for line in lines {
        if kept.len() < most.0 && line.len() < room {
            room -= line.len() + 1;
            kept.push(line);
        } else {
            left += 1;
        }
    }
    (kept.join("\n"), left)
}

/// Memory `id`'s path among the current memories in `folders`, its place in the retired tree
/// and its file, or the refusal that says there is none; an id holding `/` could name a file
/// outside them, so it names none.
fn find(folders: &Folders, id: &str) -> Result<(PathBuf, PathBuf, Found), Error> {
    let name = format!("{id}.md");
    let inside = folders.current.iter().zip(&folders.retired);
    let safe = !id.contains('/')
        || id
            .strip_prefix("state/")
            .is_some_and(|s| crate::protocol::SessionId::try_from(s.to_owned()).is_ok())
        || id
            .strip_prefix("wiki/")
            .is_some_and(|s| page_name(s) || s == "_instructions");
    for (folder, retired) in inside.filter(|_| safe) {
        if let Ok(found) = read(&folder.join(&name), id) {
            return Ok((folder.join(&name), retired.join(&name), found));
        }
    }
    Err(refuse(&TEXT.no_memory.replace("{id}", id)))
}

/// The file at `path`, if it is a regular file: opened without waiting on a pipe, so a pipe is
/// no memory and no task.
pub(super) fn read(path: &Path, id: &str) -> std::io::Result<Found> {
    let file = File::options()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)?;
    let meta = file.metadata()?;
    if !meta.is_file() {
        return Err(ErrorKind::InvalidInput.into());
    }
    let at = written(id).map_or_else(|| meta.modified(), Ok)?;
    let mut bytes = Vec::new();
    file.take(FILE).read_to_end(&mut bytes)?;
    let (id, file) = (id.into(), String::from_utf8_lossy(&bytes).into_owned());
    Ok(Found { id, at, file })
}

/// When a ruling, fact or note was written, from the stamp in its id; a file a fetch checks out
/// is new on this disk, so its own time would make another home's old memory the newest.
fn written(id: &str) -> Option<SystemTime> {
    let (_, stamp) = id.split_once('-').filter(|_| !id.contains('/'))?;
    let at = jiff::civil::DateTime::strptime("%Y%m%d-%H%M%S", stamp.get(..15)?).ok()?;
    let at = at.to_zoned(jiff::tz::TimeZone::UTC).ok()?;
    Some(at.timestamp().into())
}

/// A file's header and its text, lines ending in LF or CRLF; a file without a header is all
/// text.
pub(super) fn split(file: &str) -> (&str, &str) {
    let rest = file
        .strip_prefix("---\n")
        .or_else(|| file.strip_prefix("---\r\n"));
    rest.and_then(|r| {
        r.split_once("\n---\n")
            .or_else(|| r.split_once("\n---\r\n"))
    })
    .unwrap_or(("", file))
}

/// The header lines of a file, each a key and its value.
pub(super) fn head(file: &str) -> impl Iterator<Item = (&str, &str)> {
    let pairs = split(file).0.lines().filter_map(|l| l.split_once(": "));
    pairs.map(|(key, value)| (key, value.trim_end()))
}

/// The value of header line `key` in `file`.
fn header<'a>(file: &'a str, key: &str) -> Option<&'a str> {
    head(file).find(|(k, _)| *k == key).map(|(_, value)| value)
}

/// Whether the memory in `file` is past its `expires` date, `today` being a date like it.
fn expired(file: &str, today: &str) -> bool {
    header(file, "expires").is_some_and(|date| date < today)
}

/// A memory's kind, from its id: `ruling-…`, `fact-…`, `state`; any other is a note.
pub(super) fn kind(id: &str) -> &str {
    if id.starts_with("wiki/") {
        return "page";
    }
    if id.starts_with("state/") {
        return "state";
    }
    match id.split_once('-').map_or(id, |(kind, _)| kind) {
        kind @ ("ruling" | "fact" | "state") => kind,
        _ => "note",
    }
}

/// Memories as search and the person's list show them: by a description, else a ruling by its
/// page line and any other by its text on one line, each at most 400 bytes.
fn hits(found: &[Found], omitted: usize) -> Value {
    let hit = |f: &Found| {
        let text = || match kind(&f.id) {
            "ruling" => rule(split(&f.file).1).to_owned(),
            _ => split(&f.file)
                .1
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" "),
        };
        let summary = header(&f.file, "description").map_or_else(text, str::to_owned);
        let at = jiff::Timestamp::try_from(f.at).map(|t| t.strftime("%Y-%m-%dT%H:%M:%SZ"));
        let at = at.map(|at| at.to_string()).unwrap_or_default();
        let summary = &summary[..summary.floor_char_boundary(400)];
        json!({"id": f.id, "kind": kind(&f.id), "summary": summary, "updated_at": at})
    };
    json!({"hits": found.iter().map(hit).collect::<Vec<_>>(), "omitted_count": omitted})
}

/// The rule line, or the agent's reading in a file written before rule lines.
fn rule(body: &str) -> &str {
    let text = body
        .rsplit_once("\"\n\nagent's reading:")
        .map_or(body, |(_, r)| r.trim_start());
    text.lines().next().unwrap_or_default()
}

/// Whether `text` holds what looks like a secret: a word starting with a known key's prefix and at
/// least 16 more of a key's characters, a key with no prefix in fal's shape (a UUID, a colon and
/// at least 32 hex digits, the key found in memory on 9 November), or a PEM private-key block.
fn secret(text: &str) -> bool {
    let key = |c: char| c.is_ascii_alphanumeric() || c == '-' || c == '_';
    let long = |w: &str, k: &String| w.starts_with(k.as_str()) && w.len() >= k.len() + 16;
    let found = |w: &str| TEXT.secret_keys.iter().any(|k| long(w, k));
    let hex = |s: &str| s.bytes().all(|b| b.is_ascii_hexdigit());
    let uuid = |s: &str| s.len() == 36 && [8, 13, 18, 23].iter().all(|&i| s.as_bytes()[i] == b'-');
    let pair = |w: &str| {
        w.split_once(':').is_some_and(|(id, secret)| {
            uuid(id) && hex(&id.replace('-', "")) && secret.len() >= 32 && hex(secret)
        })
    };
    text.contains(TEXT.secret_block.as_str())
        || text.split(|c| !key(c)).any(found)
        || text.split(|c| !key(c) && c != ':').any(pair)
}

/// Whether `c` separates words: anything but a letter or a digit.
fn apart(c: char) -> bool {
    !c.is_alphanumeric()
}

/// A page is one portable file stem, never the person's instructions or a path.
pub(super) fn page_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 100
        && name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
        && name != "_instructions"
}

/// Whether an id resolves to the memories for every project.
pub(super) fn global(folders: &Folders, id: &str) -> Result<bool, Error> {
    Ok(find(folders, id)?.0.starts_with(&folders.current[1]))
}
