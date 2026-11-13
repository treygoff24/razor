//! Shared tasks and memory: each folder is a git checkout whose commits travel only on its
//! own reference on the remote the person names, each authored as the person who made it.
//! A write runs against the remote's tasks as last fetched, and its push is the compare-and-swap:
//! when another machine pushed first, the write's commit is dropped and the write runs again.

use super::files::Owners;
use super::{Answer, Error, TEXT, blocking, fill, other, refuse};
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::fs;

/// The one reference pushed, and the remote's tasks as last fetched.
const SEEN: &str = "refs/razor/remote";
/// Seconds a fetch or a push may take: about three times the slowest GitHub took on 2026-11-03.
const NET: u64 = 60;
/// Local git may be starved on a busy machine; bound each step without borrowing network time.
const LOCAL: u64 = 120;

/// Fetch, apply and push under the caller's lock. A raced push or a refusal on an older view
/// retries against a new fetch, at most twice more. Without an operation this only refreshes.
/// A shared change with no person to commit as is refused before anything changes.
/// The bool reports remote changes even when the operation is refused.
pub(super) async fn change<F>(
    root: &Path,
    dir: &Path,
    remote: &Result<Option<String>, String>,
    reference: &str,
    op: Option<F>,
    subject: impl Fn(&Value) -> String,
) -> (Answer, bool)
where
    F: Fn() -> Answer + Send + Sync + 'static,
{
    let op = op.map(Arc::new);
    let mut synced = false;
    let answer = async {
        let shared = matches!(remote, Ok(Some(_)));
        let who = if shared { person(root).await } else { None };
        if shared && who.is_none() && op.is_some() {
            return Err(refuse(&TEXT.identity));
        }
        let mut last = Ok(Value::Null);
        for round in 0..3 {
            let fetch = round > 0 || op.is_none() || stale(dir).await;
            let url = match remote {
                Ok(Some(name)) => {
                    let who = who.as_ref();
                    let (url, changed) = fresh(root, dir, name, fetch, reference, who).await?;
                    synced |= changed;
                    url
                }
                Ok(None) => None,
                Err(why) => return Err(refuse(&fill(&TEXT.settings, &[("why", why)]))),
            };
            let Some(op) = op.clone() else { break };
            let Some(url) = url else {
                return blocking(move || op()).await;
            };
            let base = rev(dir, "HEAD").await?;
            let done = blocking(move || op()).await;
            let refused;
            (last, refused) = finish(
                dir,
                &url,
                reference,
                who.as_ref(),
                base.as_deref(),
                done,
                &subject,
            )
            .await?;
            if last.is_ok() || (refused && fetch) {
                break;
            }
        }
        last
    }
    .await;
    (answer, synced)
}

/// Git in the task folder `dir` for at most `secs` seconds, with no hooks and no signing, and
/// credentials sent with the first request instead of after its refusal.
async fn git(dir: &Path, secs: u64, args: &[&str]) -> Result<String, Error> {
    let fixed = "--literal-pathspecs -c core.hooksPath=/dev/null -c commit.gpgSign=false -c http.proactiveAuth=basic";
    let args: Vec<&str> = fixed.split(' ').chain(args.iter().copied()).collect();
    let lock = dir.join(".git/index.lock");
    let index = |why: &str| why.contains("index.lock': File exists");
    let mut ran = crate::children::git::timed(dir, secs, &args).await;
    if let Err(why) = &ran
        && index(&why.to_string())
    {
        let age = fs::metadata(&lock).await.and_then(|m| m.modified());
        if let Some(age) = age.ok().and_then(|at| at.elapsed().ok())
            && age > Duration::from_secs(600)
        {
            match fs::remove_file(&lock).await {
                Ok(()) => (),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
                Err(e) => return Err(other("removing an old git index lock")(e)),
            }
            eprintln!(
                "razor: removed index.lock in {} after {} seconds",
                dir.display(),
                age.as_secs()
            );
            ran = crate::children::git::timed(dir, secs, &args).await;
        }
    }
    ran.map_err(|e| {
        let mut why = e.to_string();
        if index(&why) {
            let quoted = format!("'{}'", lock.display().to_string().replace('\'', "'\\''"));
            why += &fill(&TEXT.git_lock, &[("lock", &quoted)]);
        } else if why.contains(".lock") && why.contains("File exists") {
            why += &TEXT.git_lock_other;
        }
        refuse(&why)
    })
}

/// Git in `dir` running a command that commits, as `who`, the person, when known.
async fn authored(dir: &Path, who: Who<'_>, args: &[&str]) -> Result<String, Error> {
    let set = who.map(|(name, email)| [format!("user.name={name}"), format!("user.email={email}")]);
    let flags = set.iter().flatten().flat_map(|s| ["-c", s.as_str()]);
    let args: Vec<&str> = flags.chain(args.iter().copied()).collect();
    git(dir, LOCAL, &args).await
}

/// The person a shared change is committed as, their name and email.
type Who<'a> = Option<&'a (String, String)>;

/// Git's `user.name` and `user.email` in the project's checkout at `root`, included files and
/// all; none when either is unset.
pub(super) async fn person(root: &Path) -> Option<(String, String)> {
    let get = async |key| git(root, LOCAL, &["config", key]).await.ok();
    let (name, email) = (get("user.name").await?, get("user.email").await?);
    (!name.is_empty() && !email.is_empty()).then_some((name, email))
}

/// Who added each ruling to the memory checkout `dir`, by its id, in one history query: their
/// name, and whether they are the person in `root`'s checkout. Renames are not followed, since a
/// replacement holding its predecessor's words is a ruling of its own; of two additions of one
/// file, the latest counts.
pub(super) async fn owners(root: &Path, dir: &Path) -> Owners {
    let me = person(root).await.map(|(_, email)| email);
    let log = "--no-literal-pathspecs log --no-renames --diff-filter=A --name-only --format=%x00%an%x00%ae -- *ruling-*.md";
    let log = git(dir, LOCAL, &log.split(' ').collect::<Vec<_>>()).await;
    let (mut owners, mut author) = (Owners::new(), None);
    for line in log.unwrap_or_default().lines() {
        if let Some((name, email)) = line.strip_prefix('\0').and_then(|l| l.split_once('\0')) {
            author = Some((name.to_owned(), me.as_deref() == Some(email)));
        } else if let (Some(author), Some(id)) = (&author, Path::new(line).file_stem()) {
            let id = id.to_string_lossy().into_owned();
            owners.entry(id).or_insert_with(|| author.clone());
        }
    }
    owners
}

/// Everyone who has committed to the shared folder `dir`, each by name and email.
pub(super) async fn people(dir: &Path) -> Vec<(String, String)> {
    let log = git(dir, LOCAL, &["log", "--format=%an%x00%ae"]).await;
    let log = log.unwrap_or_default();
    let people: BTreeSet<_> = log.lines().filter_map(|l| l.split_once('\0')).collect();
    let owned = |(name, email): (&str, &str)| (name.to_owned(), email.to_owned());
    people.into_iter().map(owned).collect()
}

/// The first commit of `revision` in `dir`, which every folder joined to that history holds.
pub(super) async fn first(dir: &Path, revision: &str) -> Result<Option<String>, Error> {
    let roots = git(dir, LOCAL, &["rev-list", "--max-parents=0", revision]).await?;
    Ok(roots.lines().last().map(str::to_owned))
}

async fn roots(dir: &Path, revision: &str) -> Result<BTreeSet<String>, Error> {
    let roots = git(dir, LOCAL, &["rev-list", "--max-parents=0", revision]).await?;
    Ok(roots.lines().map(str::to_owned).collect())
}

/// Whether the origin of the checkout at `root` holds shared memory or tasks.
pub(super) async fn offered(root: &Path) -> bool {
    let refs = "ls-remote origin refs/razor/memory refs/razor/tasks".split(' ');
    let found = git(root, NET, &refs.collect::<Vec<_>>()).await;
    found.is_ok_and(|refs| !refs.is_empty())
}

/// `dir` brought up to the remote's tasks, fetched anew when `get`, else as last fetched, and
/// the URL of that remote: `name`, a remote of the project's checkout at `root`, else itself a URL
/// or path git reaches, a relative path, the remote's or `name`'s, read from `root` as git there
/// would read it; the first time, `dir` becomes a checkout. Edits made outside a write, the
/// folder's own tasks before it was shared among them, are committed and kept on top of the
/// remote's; when they touch a file the remote's tasks changed, nothing changes and the call is
/// refused with what to do. Unrelated local files stay unshared, reported by an absent URL.
pub(super) async fn fresh(
    root: &Path,
    dir: &Path,
    name: &str,
    get: bool,
    reference: &str,
    who: Who<'_>,
) -> Result<(Option<String>, bool), Error> {
    let url = git(root, LOCAL, &["remote", "get-url", "--", name]).await;
    let url = url.unwrap_or_else(|_| name.to_owned());
    // A path, no `:` before any `/`, is the checkout's; git would read it from `dir`, where it runs.
    // One under `~` is git's to expand from the home folder.
    let path = !url.starts_with('~') && url.find(':').is_none_or(|c| url[..c].contains('/'));
    let joined = root.join(&url);
    let url = joined.to_str().filter(|_| path).map_or(url, str::to_owned);
    if fs::metadata(dir.join(".git")).await.is_err() {
        let made = fs::create_dir_all(dir).await;
        made.map_err(other("making a tasks folder"))?;
        git(dir, LOCAL, &["init", "-q"]).await?;
    }
    let seen = if get { rev(dir, SEEN).await? } else { None };
    let tree = if get {
        rev(dir, "HEAD^{tree}").await?
    } else {
        None
    };
    // The pattern matches nothing on a remote with no tasks yet, rather than failing.
    let refs = format!("+{reference}*:{SEEN}*");
    if get {
        let fetched = git(dir, NET, &["fetch", "-q", "--no-tags", &url, &refs]).await;
        fetched.map_err(|e| refuse(&fill(&TEXT.offline, &[("why", &e)])))?;
    }
    let (seen_head, head) = (rev(dir, SEEN).await?, rev(dir, "HEAD").await?);
    if unrelated(dir, seen_head.as_deref(), head.as_deref()).await? {
        return Ok((None, false));
    }
    let status = git(dir, LOCAL, &["status", "--porcelain"]).await?;
    if !status.is_empty() {
        git(dir, LOCAL, &["add", "-A"]).await?;
        authored(dir, who, &["commit", "-q", "-m", "edited by hand"]).await?;
    }
    match (seen_head.is_some(), head.is_some() || !status.is_empty()) {
        (false, _) => Ok(()),
        // Nothing here yet, not even an untracked file, so taking the remote's loses nothing.
        (true, false) => git(dir, LOCAL, &["reset", "-q", "--hard", SEEN])
            .await
            .map(|_| ()),
        (true, true) => match authored(dir, who, &["rebase", "-q", SEEN]).await {
            Ok(_) => Ok(()),
            Err(why) => {
                let _ = git(dir, LOCAL, &["rebase", "--abort"]).await;
                Err(unfinished(dir, &why, &TEXT.rebase))
            }
        },
    }?;
    let now = if get { rev(dir, SEEN).await? } else { None };
    let changed = now.is_some() && now != seen && tree != rev(dir, "HEAD^{tree}").await?;
    Ok((Some(url), changed))
}

/// Whether local files have no history in common with the last fetched, nonempty remote.
/// No fetch or file changes: the startup page and the refresh use the same decision.
pub(super) async fn unjoined(dir: &Path) -> Result<bool, Error> {
    let (seen, head) = (rev(dir, SEEN).await?, rev(dir, "HEAD").await?);
    unrelated(dir, seen.as_deref(), head.as_deref()).await
}

async fn unrelated(dir: &Path, seen: Option<&str>, head: Option<&str>) -> Result<bool, Error> {
    let Some(seen) = seen else {
        return Ok(false);
    };
    if let Some(head) = head
        && git(dir, LOCAL, &["merge-base", "--is-ancestor", seen, head])
            .await
            .is_ok()
    {
        return Ok(false);
    }
    let mut entries = fs::read_dir(dir).await.map_err(other("reading a store"))?;
    let mut own = false;
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(other("reading a store"))?
    {
        own = entry.file_name() != ".git";
        if own {
            break;
        }
    }
    if !own {
        return Ok(false);
    }
    let Some(head) = head else {
        return Ok(true);
    };
    Ok(roots(dir, head).await? != roots(dir, seen).await?)
}

/// Applies no further edits after a refusal. Failed edits or commits restore the changed paths;
/// a completed commit is undone against the captured base, with git refusing later outside edits.
async fn finish(
    dir: &Path,
    url: &str,
    reference: &str,
    who: Who<'_>,
    base: Option<&str>,
    done: Answer,
    subject: &impl Fn(&Value) -> String,
) -> Result<(Answer, bool), Error> {
    let status = git(
        dir,
        LOCAL,
        &["status", "--porcelain", "-z", "--no-renames", "-uall"],
    )
    .await
    .map_err(|why| unfinished(dir, &why, &TEXT.rollback))?;
    let refused = done.is_err();
    let mut pushing = false;
    let result = async {
        let done = done?;
        git(dir, LOCAL, &["add", "-A"]).await?;
        let subject = subject(&done);
        authored(dir, who, &["commit", "-q", "--allow-empty", "-m", &subject]).await?;
        pushing = true;
        git(dir, NET, &["push", "-q", url, &format!("HEAD:{reference}")]).await?;
        Ok(done)
    }
    .await;
    let Err(e) = result else {
        return Ok((result, refused));
    };
    let why = e.to_string();
    let unknown = pushing && why.starts_with(crate::children::git::DEADLINE);
    let why = if unknown {
        fill(&TEXT.push_unknown, &[("why", &e)])
    } else {
        why
    };
    rollback(dir, base, &status).await.map_err(|rollback| {
        unfinished(dir, &refuse(&format!("{why}; {rollback}")), &TEXT.rollback)
    })?;
    if refused || pushing && (why.contains("[rejected]") || why.contains("cannot lock ref")) {
        return Ok((Err(e), refused));
    }
    Err(refuse(&if unknown {
        why
    } else {
        fill(&TEXT.not_written, &[("why", &e)])
    }))
}

async fn rev(dir: &Path, name: &str) -> Result<Option<String>, Error> {
    crate::children::git::revision(dir, LOCAL, name)
        .await
        .map_err(|why| refuse(&why.to_string()))
}

/// Restore the pre-write revision, including an unborn branch. No whole-tree reset for edits
/// that never committed; status names every changed path, with renames split into two entries.
async fn rollback(dir: &Path, base: Option<&str>, status: &str) -> Result<(), Error> {
    let head = rev(dir, "HEAD").await?;
    if let Some(base) = base.filter(|base| head.as_deref() != Some(base)) {
        git(dir, LOCAL, &["reset", "-q", "--keep", base]).await?;
        return Ok(());
    }
    if status.len() >= 64 * 1024 || (!status.is_empty() && !status.ends_with('\0')) {
        return Err(refuse(&TEXT.paths_incomplete));
    }
    for entry in status.split_terminator('\0') {
        let path = &entry[3..];
        if let Some(base) = base.filter(|_| !entry.starts_with("??")) {
            git(dir, LOCAL, &["restore", "-s", base, "-SW", "--", path]).await?;
        } else {
            // An unborn store whose commit finished keeps git's check for later outside edits.
            let remove = if head.as_deref() == base { "-f" } else { "-q" };
            let args = ["rm", "--ignore-unmatch", remove, "--", path];
            git(dir, LOCAL, &args).await?;
            match fs::remove_file(dir.join(path)).await {
                Ok(()) => (),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
                Err(e) => return Err(other("removing a refused write")(e)),
            }
        }
    }
    if let Some(head) = head.filter(|_| base.is_none()) {
        git(dir, LOCAL, &["update-ref", "-d", "HEAD", &head]).await?;
    }
    Ok(())
}

fn unfinished(dir: &Path, why: &Error, sentence: &str) -> Error {
    refuse(&fill(sentence, &[("dir", &dir.display()), ("why", why)]))
}

/// Whether the last fetch is over 30 seconds old.
pub(super) async fn stale(dir: &Path) -> bool {
    let at = fs::metadata(dir.join(".git/FETCH_HEAD")).await;
    let at = at.and_then(|m| m.modified()).ok();
    let age = at.and_then(|at| at.elapsed().ok());
    age.is_none_or(|age| age > Duration::from_secs(30))
}
