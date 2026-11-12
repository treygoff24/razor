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
                    Some(url)
                }
                Ok(None) => None,
                Err(why) => return Err(refuse(&fill(&TEXT.settings, &[("why", why)]))),
            };
            let Some(op) = op.clone() else { break };
            match (url, blocking(move || op()).await) {
                (Some(_), Err(_)) if !fetch => {}
                (Some(url), Ok(done)) => {
                    last = push(dir, &url, &subject(&done), reference, who.as_ref())
                        .await?
                        .map(|()| done);
                    if last.is_ok() {
                        break;
                    }
                }
                (_, done) => return done,
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
    let fixed = "-c core.hooksPath=/dev/null -c commit.gpgSign=false -c http.proactiveAuth=basic";
    let args: Vec<&str> = fixed.split(' ').chain(args.iter().copied()).collect();
    let ran = crate::children::git::timed(dir, secs, &args).await;
    ran.map_err(|e| refuse(&e.to_string()))
}

/// Git in `dir` running a command that commits, as `who`, the person, when known.
async fn authored(dir: &Path, who: Who<'_>, args: &[&str]) -> Result<String, Error> {
    let set = who.map(|(name, email)| [format!("user.name={name}"), format!("user.email={email}")]);
    let flags = set.iter().flatten().flat_map(|s| ["-c", s.as_str()]);
    let args: Vec<&str> = flags.chain(args.iter().copied()).collect();
    git(dir, 10, &args).await
}

/// The person a shared change is committed as, their name and email.
type Who<'a> = Option<&'a (String, String)>;

/// Git's `user.name` and `user.email` in the project's checkout at `root`, included files and
/// all; none when either is unset.
pub(super) async fn person(root: &Path) -> Option<(String, String)> {
    let get = async |key| git(root, 10, &["config", key]).await.ok();
    let (name, email) = (get("user.name").await?, get("user.email").await?);
    (!name.is_empty() && !email.is_empty()).then_some((name, email))
}

/// Who added each ruling to the memory checkout `dir`, by its id, in one history query: their
/// name, and whether they are the person in `root`'s checkout. Renames are not followed, since a
/// replacement holding its predecessor's words is a ruling of its own; of two additions of one
/// file, the latest counts.
pub(super) async fn owners(root: &Path, dir: &Path) -> Owners {
    let me = person(root).await.map(|(_, email)| email);
    let log =
        "log --no-renames --diff-filter=A --name-only --format=%x00%an%x00%ae -- *ruling-*.md";
    let log = git(dir, 10, &log.split(' ').collect::<Vec<_>>()).await;
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
    let log = git(dir, 10, &["log", "--format=%an%x00%ae"]).await;
    let log = log.unwrap_or_default();
    let people: BTreeSet<_> = log.lines().filter_map(|l| l.split_once('\0')).collect();
    let owned = |(name, email): (&str, &str)| (name.to_owned(), email.to_owned());
    people.into_iter().map(owned).collect()
}

/// The first commit of the shared folder `dir`, which every folder joined to its remote holds.
pub(super) async fn first(dir: &Path) -> Option<String> {
    let roots = git(dir, 10, &["rev-list", "--max-parents=0", "HEAD"])
        .await
        .ok()?;
    roots.lines().last().map(str::to_owned)
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
/// refused with what to do.
pub(super) async fn fresh(
    root: &Path,
    dir: &Path,
    name: &str,
    get: bool,
    reference: &str,
    who: Who<'_>,
) -> Result<(String, bool), Error> {
    let url = git(root, 10, &["remote", "get-url", "--", name]).await;
    let url = url.unwrap_or_else(|_| name.to_owned());
    // A path, no `:` before any `/`, is the checkout's; git would read it from `dir`, where it runs.
    // One under `~` is git's to expand from the home folder.
    let path = !url.starts_with('~') && url.find(':').is_none_or(|c| url[..c].contains('/'));
    let joined = root.join(&url);
    let url = joined.to_str().filter(|_| path).map_or(url, str::to_owned);
    if fs::metadata(dir.join(".git")).await.is_err() {
        let made = fs::create_dir_all(dir).await;
        made.map_err(other("making a tasks folder"))?;
        git(dir, 10, &["init", "-q"]).await?;
    }
    let rev = async |name| {
        git(dir, 10, &["rev-parse", "-q", "--verify", name])
            .await
            .ok()
    };
    let seen = if get { rev(SEEN).await } else { None };
    let tree = if get { rev("HEAD^{tree}").await } else { None };
    // The pattern matches nothing on a remote with no tasks yet, rather than failing.
    let refs = format!("+{reference}*:{SEEN}*");
    if get {
        let fetched = git(dir, NET, &["fetch", "-q", "--no-tags", &url, &refs]).await;
        fetched.map_err(|e| refuse(&fill(&TEXT.offline, &[("why", &e)])))?;
    }
    if !git(dir, 10, &["status", "--porcelain"]).await?.is_empty() {
        git(dir, 10, &["add", "-A"]).await?;
        authored(dir, who, &["commit", "-q", "-m", "edited by hand"]).await?;
    }
    match (rev(SEEN).await.is_some(), rev("HEAD").await.is_some()) {
        (false, _) => Ok(()),
        // Nothing here yet, not even an untracked file, so taking the remote's loses nothing.
        (true, false) => git(dir, 10, &["reset", "-q", "--hard", SEEN])
            .await
            .map(|_| ()),
        (true, true) if authored(dir, who, &["rebase", "-q", SEEN]).await.is_ok() => Ok(()),
        (true, true) => {
            let _ = git(dir, 10, &["rebase", "--abort"]).await;
            Err(refuse(&fill(&TEXT.conflict, &[("dir", &dir.display())])))
        }
    }?;
    let now = if get { rev(SEEN).await } else { None };
    let changed = now.is_some() && now != seen && tree != rev("HEAD^{tree}").await;
    Ok((url, changed))
}

/// Commits the change just applied with `subject` and pushes it. A failed push drops the commit
/// again, leaving the folder as the remote had it; when the remote moved meanwhile, the answer is
/// git's words inside `Ok`, so the write may run again from a fetch, and otherwise it is the
/// remote out of reach. When a file the commit changed was edited meanwhile, the commit stays,
/// nothing is pushed, and the answer is the refusal that says what to do.
pub(super) async fn push(
    dir: &Path,
    url: &str,
    subject: &str,
    reference: &str,
    who: Who<'_>,
) -> Result<Result<(), Error>, Error> {
    git(dir, 10, &["add", "-A"]).await?;
    authored(dir, who, &["commit", "-q", "--allow-empty", "-m", subject]).await?;
    let pushed = git(dir, NET, &["push", "-q", url, &format!("HEAD:{reference}")]).await;
    let Err(e) = pushed else { return Ok(Ok(())) };
    let kept = git(dir, 10, &["reset", "-q", "--keep", "HEAD~1"]).await;
    kept.map_err(|_| refuse(&fill(&TEXT.conflict, &[("dir", &dir.display())])))?;
    // Git's marks for a push that lost to another: the remote holds commits this folder lacks,
    // or took them while this push was arriving.
    let lost = e.to_string();
    match lost.contains("[rejected]") || lost.contains("cannot lock ref") {
        true => Ok(Err(e)),
        false => Err(refuse(&fill(&TEXT.offline, &[("why", &e)]))),
    }
}

/// Whether the last fetch is over 30 seconds old.
pub(super) async fn stale(dir: &Path) -> bool {
    let at = fs::metadata(dir.join(".git/FETCH_HEAD")).await;
    let at = at.and_then(|m| m.modified()).ok();
    let age = at.and_then(|at| at.elapsed().ok());
    age.is_none_or(|age| age > Duration::from_secs(30))
}
