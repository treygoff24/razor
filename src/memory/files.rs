//! Razor's own memory (docs/design/memory.md): one Markdown file per memory in the server's
//! home, in `memory/all/` for every project and in `memory/projects/<the project's path>/` for
//! one, and a retired memory at the same place under `memory/retired/`. A file's name says its
//! kind, state notes are `state/<session>.md` with their merged note at `state.md`, and a header between `---` lines holds its
//! source, any expiry and a fact's or note's description. Nothing is held between calls, and
//! everything here runs on a blocking thread. Task files share the reader, header and writer.

use super::{Answer, Error, Kind, TEXT, Write, other, refuse, words};
use serde_json::{Value, json};
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

/// The folders of one project's memories and of every project's, each pair in that order.
#[derive(Clone)]
pub(super) struct Folders {
    pub(super) current: [PathBuf; 2],
    retired: [PathBuf; 2],
}

/// One memory or task file as a read found it: at most `FILE` bytes, and when it last changed.
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
    let (current, retired) = (both(dir.into()), both(dir.join("retired")));
    Folders { current, retired }
}

/// The page's memory part: the state note, the rulings (standing ones first, then those with an
/// expiry, each newest first) and the descriptions of facts and notes, newest first, then how
/// many more there are.
pub(super) fn page(folders: &Folders) -> Result<(Option<String>, String, String), Error> {
    let state = state(folders);
    let standing = |f: &Found| Some(usize::from(header(&f.file, "expires").is_none()));
    let rulings = best(folders, KEEP, |id| kind(id) == "ruling", standing)?.0;
    let line = |f: &Found| {
        let until = header(&f.file, "expires").map(|d| TEXT.until.replace("{date}", d));
        let said = words(split(&f.file).1);
        format!("- {}{said} ({})", until.unwrap_or_default(), f.id)
    };
    let rulings = fit(rulings.iter().map(line), RULINGS).0;
    Ok((state, rulings, known(folders, KNOWN)?))
}

/// The current state note, if present and unexpired.
pub(super) fn state(folders: &Folders) -> Option<String> {
    let merged = read(&folders.current[0].join("state.md"), "state").ok();
    let since = merged
        .as_ref()
        .map(|f| f.at)
        .unwrap_or(SystemTime::UNIX_EPOCH);
    let pick = |id: &str| id.starts_with("state/");
    let notes = best(folders, KEEP, pick, |f| (f.at > since).then_some(0))
        .ok()?
        .0;
    let today = jiff::Zoned::now().date().to_string();
    let merged = merged.filter(|f| !expired(&f.file, &today));
    let bodies = merged.iter().chain(&notes).map(|f| split(&f.file).1);
    let lines = bodies.flat_map(str::lines).map(str::to_owned);
    let text = fit(lines, (usize::MAX, super::STATE + 1)).0;
    (!text.is_empty()).then_some(text)
}

/// The descriptions of facts and notes, newest first, in at most `most` lines and bytes, then a
/// line counting the rest: the page's list, and the night's when longer.
pub(super) fn known(folders: &Folders, most: (usize, usize)) -> Result<String, Error> {
    let since = fs::read_to_string(folders.current[0].join(".dream"))
        .ok()
        .and_then(|s| s.lines().next().and_then(|s| s.parse::<u64>().ok()))
        .map(|ms| SystemTime::UNIX_EPOCH + std::time::Duration::from_millis(ms))
        .unwrap_or(SystemTime::UNIX_EPOCH);
    let pick =
        |id: &str| id != "wiki/_instructions" && matches!(kind(id), "fact" | "note" | "page");
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
pub(super) fn write(folders: &Folders, kind: Kind, w: Write) -> Answer {
    if let Some(old) = &w.replaces {
        let (path, _, _) = find(folders, old)?;
        let expected = &folders.current[usize::from(w.all)];
        if !path.starts_with(expected) {
            return Err(refuse(&TEXT.scope));
        }
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
    let line = |key, value: Option<String>| value.map(|v| format!("{key}: {v}\n"));
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
            .as_deref()
            .is_some_and(|id| id.starts_with("wiki/"));
        if count >= 200 + usize::from(replacing_page) {
            return Err(refuse(&TEXT.pages_full));
        }
    }
    let tail = line("expires", w.expires).unwrap_or_default();
    let tail = tail + &line("name", w.name).unwrap_or_default();
    let tail = tail + &line("checked", w.checked).unwrap_or_default();
    let tail = tail + &line("description", w.description).unwrap_or_default();
    let tail = tail
        + &w.cites
            .iter()
            .map(|c| format!("cites: {c}\n"))
            .collect::<String>();
    let head = format!("---\nsource: {}\n{tail}---\n", w.source);
    let file = head + &w.text;
    if file.len() as u64 > FILE {
        return Err(refuse(&TEXT.file_long));
    }
    let path = folder.join(format!("{id}.md"));
    let temp = stage(path.parent().unwrap_or(folder), &file)?;
    if let Some(Err(e)) = w.replaces.as_ref().map(|old| retire(folders, old, false)) {
        let _ = fs::remove_file(&temp);
        return Err(e);
    }
    fs::rename(temp, path).map_err(other("publishing a memory file"))?;
    Ok(json!({"id": id, "replaced": w.replaces}))
}

/// Moves memory `id` into the retired tree, where search, get and the page no longer see it;
/// with `forget`, the person's word, deletes it instead, words and all. It is one rename or
/// removal, so of calls that retire one memory at once, one succeeds and the others are refused.
pub(super) fn retire(folders: &Folders, id: &str, forget: bool) -> Answer {
    if id == "wiki/_instructions" {
        return Err(refuse(&TEXT.instructions));
    }
    let (path, to, _) = find(folders, id)?;
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
    Ok(json!({ "retired": id }))
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
    let at = meta.modified()?;
    let mut bytes = Vec::new();
    file.take(FILE).read_to_end(&mut bytes)?;
    let (id, file) = (id.into(), String::from_utf8_lossy(&bytes).into_owned());
    Ok(Found { id, at, file })
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
        let text = || words(split(&f.file).1);
        let summary = header(&f.file, "description").map_or_else(text, str::to_owned);
        let at = jiff::Timestamp::try_from(f.at).map(|t| t.strftime("%Y-%m-%dT%H:%M:%SZ"));
        let at = at.map(|at| at.to_string()).unwrap_or_default();
        let summary = &summary[..summary.floor_char_boundary(400)];
        json!({"id": f.id, "kind": kind(&f.id), "summary": summary, "updated_at": at})
    };
    json!({"hits": found.iter().map(hit).collect::<Vec<_>>(), "omitted_count": omitted})
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
