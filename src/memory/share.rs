//! Shared tasks and memory: each folder is a git checkout whose commits travel only on its
//! own reference on the remote the person names.
//! A write runs against the remote's tasks as last fetched, and its push is the compare-and-swap:
//! when another machine pushed first, the write's commit is dropped and the write runs again.

use super::{Answer, Error, TEXT, blocking, fill, other, refuse};
use serde_json::Value;
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
        let mut last = Ok(Value::Null);
        for round in 0..3 {
            let fetch = round > 0 || stale(dir).await;
            let url = match remote {
                Ok(Some(name)) => {
                    let (url, changed) = fresh(root, dir, name, fetch, reference).await?;
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
                    last = push(dir, &url, &subject(&done), reference)
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

/// Git in the task folder `dir` for at most `secs` seconds, with no hooks, no signing, Razor as
/// the author, and credentials sent with the first request instead of after its refusal.
async fn git(dir: &Path, secs: u64, args: &[&str]) -> Result<String, Error> {
    let fixed = "-c core.hooksPath=/dev/null -c commit.gpgSign=false -c http.proactiveAuth=basic \
                 -c user.name=Razor -c user.email=razor@localhost";
    let args: Vec<&str> = fixed.split(' ').chain(args.iter().copied()).collect();
    let ran = crate::children::git::timed(dir, secs, &args).await;
    ran.map_err(|e| refuse(&e.to_string()))
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
        git(dir, 10, &["commit", "-q", "-m", "edited by hand"]).await?;
    }
    match (rev(SEEN).await.is_some(), rev("HEAD").await.is_some()) {
        (false, _) => Ok(()),
        // Nothing here yet, not even an untracked file, so taking the remote's loses nothing.
        (true, false) => git(dir, 10, &["reset", "-q", "--hard", SEEN])
            .await
            .map(|_| ()),
        (true, true) if git(dir, 10, &["rebase", "-q", SEEN]).await.is_ok() => Ok(()),
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
) -> Result<Result<(), Error>, Error> {
    git(dir, 10, &["add", "-A"]).await?;
    git(dir, 10, &["commit", "-q", "--allow-empty", "-m", subject]).await?;
    let pushed = git(dir, NET, &["push", "-q", url, &format!("HEAD:{reference}")]).await;
    let Err(e) = pushed else { return Ok(Ok(())) };
    let kept = git(dir, 10, &["reset", "-q", "--keep", "HEAD~1"]).await;
    kept.map_err(|_| refuse(&fill(&TEXT.conflict, &[("dir", &dir.display())])))?;
    // Git's mark for a push refused because the remote holds commits this folder lacks.
    match e.to_string().contains("[rejected]") {
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
