//! Installed release maintenance, using the generated installer's selection and retention.
use super::{Error, other};
use crate::{
    config::{Config, INSTALL_DIR},
    shell::Group,
};
use std::{
    io,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::{fs, process::Command};

const INSTALLER_LIMIT: usize = 64 * 1024;

fn version(tag: &str) -> Option<[u64; 3]> {
    let mut parts = tag.split('.');
    let mut numbers = [0; 3];
    for number in &mut numbers {
        let part = parts.next()?;
        if part.is_empty()
            || (part.len() > 1 && part.starts_with('0'))
            || !part.bytes().all(|b| b.is_ascii_digit())
        {
            return None;
        }
        *number = part.parse().ok()?;
    }
    parts.next().is_none().then_some(numbers)
}

pub(super) async fn installation() -> Option<(PathBuf, String)> {
    let dir = INSTALL_DIR.as_ref()?;
    let data = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))?;
    let root = fs::canonicalize(data.join("razor")).await.ok()?;
    let tag = dir.file_name()?.to_str()?;
    (dir.parent() == Some(root.as_path())
        && version(tag).is_some()
        && fs::try_exists(root.join("current/razor")).await.ok()?)
    .then(|| (root, tag.to_string()))
}

async fn selected(root: &Path) -> Option<String> {
    let selected = fs::canonicalize(root.join("current")).await.ok()?;
    let tag = selected.file_name()?.to_str()?;
    (selected.parent() == Some(root) && version(tag).is_some()).then(|| tag.to_string())
}

/// The newer selected release awaiting this process's restart; absent on source installations.
pub(crate) async fn release_ready() -> Option<String> {
    let (root, running) = installation().await?;
    let tag = selected(&root).await?;
    (version(&tag)? > version(&running)?).then_some(tag)
}

fn action(mark: &str) -> Option<([u64; 3], &str)> {
    let (since, sentence) = mark.split_once(' ')?;
    (!sentence.trim().is_empty()).then_some((version(since)?, sentence))
}

fn held(mark: &str) -> String {
    let sentence = action(mark).map_or(mark, |(_, sentence)| sentence);
    format!(
        "Razor needs your action: {sentence} Run razor update to install the latest release, then razor restart."
    )
}

/// A pending action, retained offline and hidden once the selected version meets its mark.
pub(crate) async fn release_action() -> Option<String> {
    let (root, running) = installation().await?;
    let floor = version(&running).max(selected(&root).await.as_deref().and_then(version));
    let mark = fs::read_to_string(root.join("release-action")).await.ok()?;
    action(&mark)
        .is_none_or(|(since, _)| Some(since) > floor)
        .then(|| held(&mark))
}

/// The selected release binary when starting a server; source installs keep their own binary.
pub async fn selected_executable() -> io::Result<PathBuf> {
    installation()
        .await
        .map_or_else(std::env::current_exe, |(root, _)| {
            Ok(root.join("current/razor"))
        })
}

/// Install a newer stable release through the embedded installer, never restarting.
/// Selection and retention belong to the installer; concurrent updates are refused.
/// A background caller supplies its notification settings. None selects a release
/// at the person's request.
pub async fn update_release(config: Option<&Config>) -> Result<String, Error> {
    tokio::time::timeout(Duration::from_secs(600), update(config))
        .await
        .map_err(|_| {
            Error::Refused(
                "The release update exceeded ten minutes; run razor update to try again.".into(),
            )
        })?
}

async fn update(config: Option<&Config>) -> Result<String, Error> {
    let (root, running) = installation().await.ok_or_else(|| Error::Refused(
        "This is a source installation; update from its checkout or install a published release.".into()
    ))?;
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(root.join(".update.lock"))
        .await
        .map_err(other("opening the release update lock"))?
        .into_std()
        .await;
    lock.try_lock().map_err(|_| {
        Error::Refused(
            "Another release update is running; try razor update again when it finishes.".into(),
        )
    })?;
    let pending = root.join("release-action");
    let (latest, base, mark) = latest().await?;
    let selected = selected(&root).await;
    let floor = version(&running).max(selected.as_deref().and_then(version));
    if version(&latest) <= floor {
        let _ = fs::remove_file(&pending).await;
        let installed = selected.as_deref().unwrap_or(&running);
        return Ok(format!(
            "Razor {installed}, the latest release, is installed; run razor restart if your server started before it."
        ));
    }
    let needed = mark
        .as_deref()
        .filter(|mark| action(mark).is_none_or(|(since, _)| Some(since) > floor));
    if let Some(mark) = needed
        && let Some(config) = config
    {
        let notice = held(mark);
        let previous = fs::read_to_string(&pending).await.ok();
        let temp = pending.with_extension("tmp");
        fs::write(&temp, mark)
            .await
            .map_err(other("saving the release action"))?;
        fs::rename(&temp, &pending)
            .await
            .map_err(other("saving the release action"))?;
        if previous.as_deref().map(held).as_ref() != Some(&notice) {
            super::notify(config, notice.clone(), Default::default());
        }
        return Ok(notice);
    }
    install(&latest, &base).await?;
    let _ = fs::remove_file(&pending).await;
    if let Some(mark) = needed {
        return Ok(format!(
            "Razor {latest} is ready. {}",
            action(mark).map_or(mark, |(_, sentence)| sentence)
        ));
    }
    // The installer prints the restart instruction once on stderr.
    Ok(format!("Razor {latest} is ready."))
}

async fn latest() -> Result<(String, String, Option<String>), Error> {
    let base = std::env::var("RAZOR_RELEASE_URL")
        .unwrap_or_else(|_| "https://github.com/treygoff24/razor/releases/latest/download".into());
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(other("making the release download client"))?;
    let mut response = client
        .get(format!("{}/install.sh", base.trim_end_matches('/')))
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(other(
            "checking the latest release; run razor update to try again",
        ))?;
    let mut script = Vec::new(); // At most 64 KiB, including the generated version declaration.
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(other("reading the release installer"))?
    {
        if script.len() + chunk.len() > INSTALLER_LIMIT {
            return Err(Error::Refused(
                "The release installer exceeds 64 KiB; reinstall from the release page.".into(),
            ));
        }
        script.extend_from_slice(&chunk);
    }
    let text =
        std::str::from_utf8(&script).map_err(other("reading the release installer's text"))?;
    let declared = |name| {
        let prefix = format!("{name}=${{{name}:-");
        text.lines()
            .find_map(|line| line.strip_prefix(&prefix)?.strip_suffix('}'))
    };
    let mark = text.lines().find(|line| line.starts_with("RAZOR_ACTION="));
    let mark = mark.map(|line| {
        line.strip_prefix("RAZOR_ACTION=\"")
            .and_then(|s| s.strip_suffix('"'))
            .filter(|s| action(s).is_some())
            .unwrap_or(line)
            .to_string()
    });
    declared("RAZOR_VERSION")
        .filter(|tag| version(tag).is_some())
        .zip(declared("RAZOR_BASE_URL").filter(|base| base.starts_with("https://") || base.starts_with("http://")))
        .map(|(version, base)| (version.to_string(), base.to_string(), mark))
        .ok_or_else(|| Error::Refused(
            "The release installer has no stable version or download URL; reinstall from the release page.".into()
        ))
}

async fn install(version: &str, base: &str) -> Result<(), Error> {
    let mut child = Command::new("sh")
        .arg("-c")
        .arg(include_str!("../../scripts/install-release.sh"))
        .env("RAZOR_VERSION", version)
        .env("RAZOR_BASE_URL", base)
        .env("RAZOR_SELECT", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .process_group(0)
        .kill_on_drop(true)
        .spawn()
        .map_err(other("starting the release installer"))?;
    let mut group = Group(child.id());
    let status = child
        .wait()
        .await
        .map_err(other("waiting for the release installer"))?;
    group.0 = None;
    if !status.success() {
        return Err(Error::Refused(format!(
            "The release installer exited {status}; run razor update to try again."
        )));
    }
    Ok(())
}
