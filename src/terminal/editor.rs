//! The person's own editor, which saved views are edited in and a reply is read in; the loop
//! leaves the terminal's modes before and enters them again after.
use crate::protocol::SessionId;
use std::{io, path::Path};
use tokio::io::AsyncWriteExt;

/// What the loop hands the terminal over for next.
pub enum Editing {
    Views,
    Reply(String),
}

/// Runs `$VISUAL`, else `$EDITOR`, else `vi` on `path`, its words split as a shell quotes them;
/// read-only, vi, vim and nvim get `-R`.
pub async fn open(path: &Path, read_only: bool) -> io::Result<()> {
    let editor = std::env::var("VISUAL").or_else(|_| std::env::var("EDITOR"));
    let words = shlex::split(&editor.unwrap_or_else(|_| "vi".into())).unwrap_or_default();
    let Some((program, args)) = words.split_first() else {
        return Err(io::Error::other("$VISUAL or $EDITOR names no editor."));
    };
    let name = Path::new(program).file_name().and_then(|n| n.to_str());
    let flag = matches!(name, Some("vi" | "vim" | "nvim")) && read_only;
    let mut editor = tokio::process::Command::new(program);
    let editor = editor.args(args).args(flag.then_some("-R")).arg(path);
    match editor.status().await?.success() {
        true => Ok(()),
        false => Err(io::Error::other("The editor did not exit successfully.")),
    }
}

/// Opens a reply read-only from a file only the person can read, removed on return.
pub async fn read(text: &str) -> io::Result<()> {
    let path = std::env::temp_dir().join(format!("razor-reply-{}.md", SessionId::new()));
    let mut options = tokio::fs::OpenOptions::new();
    let options = options.write(true).create_new(true).mode(0o600);
    let mut file = options.open(&path).await?;
    let read = async {
        file.write_all(text.as_bytes()).await?;
        drop(file);
        open(&path, true).await
    };
    let read = read.await;
    let _ = tokio::fs::remove_file(&path).await;
    read
}
