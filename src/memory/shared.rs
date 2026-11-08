//! Memory uses the same fetch, apply and compare-and-swap push as shared tasks.
use super::{Answer, Controller, Memory, Project, share};
use std::path::Path;
const REFERENCE: &str = "refs/razor/memory";

impl Memory {
    /// Reads keep the last fetched files when the remote is unavailable.
    pub(crate) async fn memory_fresh(&self, c: &Controller, cwd: &Path) {
        let _lock = self.lock.lock().await;
        if let Ok(p) = self.files(c, cwd, Ok).await
            && let Ok(Some(name)) = &p.memory_remote
            && share::stale(&p.memory.current[0]).await
            && let Err(e) = share::fresh(&p.root, &p.memory.current[0], name, true, REFERENCE).await
        {
            eprintln!("razor: refreshing memory: {e}");
        }
    }

    /// One memory change is on the remote before it answers. A raced push retries the operation
    /// on the freshly fetched files, so the second replacement of a memory is refused.
    pub(super) async fn memory_change<F>(
        &self,
        c: &Controller,
        cwd: &Path,
        all: bool,
        op: F,
    ) -> Answer
    where
        F: Fn(&Project) -> Answer + Send + Sync + 'static,
    {
        let _lock = self.lock.lock().await;
        let p = self.files(c, cwd, Ok).await?;
        let remote = if all {
            Ok(None)
        } else {
            p.memory_remote.clone()
        };
        let project = p.clone();
        share::change(
            &p.root,
            &p.memory.current[0],
            &remote,
            REFERENCE,
            Some(move || op(&project)),
            |_| "memory changed".into(),
        )
        .await
        .0
    }
}
