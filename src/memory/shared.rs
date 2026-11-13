//! Memory uses the same fetch, apply and compare-and-swap push as shared tasks.
use super::{
    Answer, Error, Memory, Project, TEXT, blocking, files::Folders, fill, other, refuse, share,
};
use std::{collections::BTreeMap, path::PathBuf};
use tokio::sync::Mutex;
const REFERENCE: &str = "refs/razor/memory";

/// Whether each project's origin offers shared memory and tasks, false while its one probe runs:
/// at most 256 projects while the server runs.
static OFFERS: Mutex<BTreeMap<PathBuf, bool>> = Mutex::const_new(BTreeMap::new());

impl Memory {
    /// Reads keep the last fetched files when the remote is unavailable.
    pub(super) async fn memory_fresh(&self, p: &Project) {
        let Ok(Some(name)) = &p.memory_remote else {
            return;
        };
        let (lock, p, name) = (
            self.store(&self.home.join("memory")),
            p.clone(),
            name.clone(),
        );
        let _ = tokio::spawn(async move {
            let _lock = lock.lock_owned().await;
            let dir = &p.memory.current[0];
            if share::stale(dir).await {
                if let Err(e) = shareable(&p).await {
                    eprintln!("razor: refreshing memory: {e}");
                    return;
                }
                let who = share::person(&p.root).await;
                if let Err(e) =
                    share::fresh(&p.root, dir, &name, true, REFERENCE, who.as_ref()).await
                {
                    eprintln!("razor: refreshing memory: {e}");
                }
            }
        })
        .await;
    }

    /// One memory change, `op` on `p`'s memory or, with `layer`, that resident's own layer, is on
    /// the remote before it answers. A raced push retries the operation on the freshly fetched
    /// files, so the second replacement of a memory is refused.
    pub(super) async fn memory_change<F>(
        &self,
        p: &Project,
        all: bool,
        layer: Option<&str>,
        op: F,
    ) -> Answer
    where
        F: Fn(&Folders) -> Answer + Send + Sync + 'static,
    {
        let lock = self.store(&self.home.join("memory"));
        let (p, layer) = (p.clone(), layer.map(str::to_owned));
        tokio::spawn(async move {
            let _lock = lock.lock_owned().await;
            let remote = if all { &Ok(None) } else { &p.memory_remote };
            if matches!(remote, Ok(Some(_))) {
                shareable(&p).await?;
            }
            let memory = owned(p.clone()).await.memory.layer(layer.as_deref());
            let (dir, op) = (&p.memory.current[0], Some(move || op(&memory)));
            let subject = |_: &_| "memory changed".into();
            let (answer, _) = share::change(&p.root, dir, remote, REFERENCE, op, subject).await;
            answer
        })
        .await
        .map_err(super::other("waiting for the memory write"))?
    }
}

/// Until project folders are isolated, never share directories outside memory's own layout.
/// Stores under state/, wiki/ or residents/ can be indistinguishable from owned memory.
async fn shareable(p: &Project) -> Result<(), Error> {
    let dir = p.memory.current[0].clone();
    blocking(move || {
        let own: &[&str] = &["state", "wiki", "residents", ".git"];
        for (sub, allowed) in [("", own), ("state", &[]), ("wiki", &[])] {
            let entries = match std::fs::read_dir(dir.join(sub)) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                entries => entries.map_err(other("checking shared memory folders"))?,
            };
            for entry in entries {
                let entry = entry.map_err(other("checking a shared memory folder"))?;
                let path = entry.path();
                let kind = entry
                    .file_type()
                    .map_err(other("checking a shared memory path"))?;
                if kind.is_dir() && !allowed.iter().any(|name| entry.file_name() == *name) {
                    return Err(refuse(&fill(
                        &TEXT.nested_memory,
                        &[("folder", &path.display())],
                    )));
                }
            }
        }
        Ok(())
    })
    .await
}

/// `p` with who added each of its rulings, when its memory is shared: one history query, local.
pub(super) async fn owned(mut p: Project) -> Project {
    if matches!(p.memory_remote, Ok(Some(_))) {
        p.memory.owners = share::owners(&p.root, &p.memory.current[0]).await;
    }
    p
}

/// Whether `p`'s origin offers shared memory and tasks, as a probe in the background found. The
/// first look at a project that may join starts that probe, and nothing waits on it.
pub(super) async fn offered(p: &Project) -> bool {
    let mut offers = OFFERS.lock().await;
    if p.join && offers.len() < 256 && !offers.contains_key(&p.root) {
        let root = p.root.clone();
        offers.insert(root.clone(), false);
        tokio::spawn(async move {
            let found = share::offered(&root).await;
            OFFERS.lock().await.insert(root, found);
        });
    }
    p.join && offers.get(&p.root) == Some(&true)
}
