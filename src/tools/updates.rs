//! Daily machine skill maintenance; Git and the pinned skills CLI retain their own provenance.
use crate::shell::Group;
use serde_json::Value;
use std::{
    collections::BTreeSet,
    io,
    path::{Component, Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::{fs, io::AsyncReadExt, process::Command};

/// Update eligible machine skills once, serially under the OS user's nonblocking lock.
/// Overrides belong to this Razor home; the pack and project folders are never written.
pub(crate) async fn update_skills(
    folders: Vec<PathBuf>,
    overrides: PathBuf,
    pack: Option<PathBuf>,
) {
    let result =
        tokio::time::timeout(Duration::from_secs(600), update(folders, overrides, pack)).await;
    match result {
        Ok(Ok(())) => {}
        Ok(Err(error)) => eprintln!("razor: skill updates: {error}"),
        Err(_) => eprintln!("razor: skill updates exceeded ten minutes"),
    }
}

fn refused(why: impl Into<String>) -> io::Error {
    io::Error::other(why.into())
}

async fn update(
    folders: Vec<PathBuf>,
    overrides: PathBuf,
    pack: Option<PathBuf>,
) -> io::Result<()> {
    let home = std::env::var_os("HOME").ok_or_else(|| refused("HOME is unavailable"))?;
    let agents = Path::new(&home).join(".agents");
    fs::create_dir_all(&agents).await?;
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(agents.join(".razor-skill-updates.lock"))
        .await?
        .into_std()
        .await;
    match lock.try_lock() {
        Ok(()) => {}
        Err(std::fs::TryLockError::WouldBlock) => {
            eprintln!("razor: skill updates skipped: another Razor home holds the lock");
            return Ok(());
        }
        Err(error) => return Err(refused(error.to_string())),
    }
    // Select the whole bounded set before any writes, including the pack's marked skills.
    let skills = discover(folders).await?;
    let starters = discover(pack.into_iter().collect()).await?;
    if skills.len() + starters.len() > 1000 {
        return Err(refused("more than 1,000 skills"));
    }
    let vendor = vendor(&agents).await;
    let (mut repositories, mut names) = (BTreeSet::new(), BTreeSet::new());
    for (name, path) in skills {
        let result = async {
            if let Some(root) = repository(&path).await? {
                if repositories.insert(root.clone()) {
                    checkout(&root).await?;
                }
            } else if vendor["skills"].get(&name).is_some() {
                names.insert(name);
            }
            Ok::<_, io::Error>(())
        }
        .await;
        if let Err(error) = result {
            eprintln!("razor: skill updates: {}: {error}", path.display());
        }
    }
    if !names.is_empty()
        && let Err(error) = cli(names, Path::new(&home)).await
    {
        eprintln!("razor: skill updates: {error}");
    }
    let mut moving = BTreeSet::new();
    for (name, path) in starters {
        if !fs::try_exists(path.join("UPSTREAM")).await.unwrap_or(false) {
            continue;
        }
        match starter(&name, &path, &overrides).await {
            Ok(false) => {}
            result => {
                moving.insert(name.clone());
                if let Err(error) = result {
                    eprintln!("razor: skill updates: starter {name}: {error}");
                }
            }
        }
    }
    retire(&overrides, &moving).await
}

async fn vendor(agents: &Path) -> Value {
    let metadata = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .map_or_else(
            || agents.join(".skill-lock.json"),
            |dir| dir.join("skills/.skill-lock.json"),
        );
    match bounded(&metadata, 1 << 20).await {
        Ok(text) => match serde_json::from_str::<Value>(&text) {
            Ok(value) if value["version"] == 3 && value["skills"].is_object() => value,
            _ => {
                eprintln!(
                    "razor: skill updates: malformed or unsupported global lock {}",
                    metadata.display()
                );
                Value::Null
            }
        },
        Err(error) if error.kind() == io::ErrorKind::NotFound => Value::Null,
        Err(error) => {
            eprintln!("razor: skill updates: {}: {error}", metadata.display());
            Value::Null
        }
    }
}

async fn repository(path: &Path) -> io::Result<Option<PathBuf>> {
    for ancestor in path.ancestors() {
        if fs::try_exists(ancestor.join(".git")).await? {
            return fs::canonicalize(git(path, &["rev-parse", "--show-toplevel"]).await?)
                .await
                .map(Some);
        }
    }
    Ok(None)
}

// The vendor prints per-skill results on stdout; both streams belong in server.log.
async fn cli(names: BTreeSet<String>, home: &Path) -> io::Result<()> {
    let mut command = Command::new("npx");
    command
        .args(["--yes", "skills@1.7.6", "update", "--global", "--yes"])
        .args(names)
        .current_dir(home)
        .stdin(Stdio::null())
        .stderr(Stdio::inherit())
        .stdout(std::io::stderr())
        .process_group(0)
        .kill_on_drop(true)
        .env("npm_config_ignore_scripts", "true")
        .env("DISABLE_TELEMETRY", "1")
        .env("NO_COLOR", "1")
        .env("LC_ALL", "C");
    tokio::time::timeout(Duration::from_secs(120), async {
        let mut child = command.spawn()?;
        let mut group = Group(child.id());
        let status = child.wait().await?;
        group.0 = None;
        eprintln!("razor: skill updates: npx: {status}");
        if !status.success() {
            return Err(refused(format!("npx exited {status}")));
        }
        Ok(())
    })
    .await
    .map_err(|_| refused("npx exceeded two minutes"))?
}

fn name_valid(name: &str) -> bool {
    name.as_bytes()
        .first()
        .is_some_and(u8::is_ascii_alphanumeric)
        && name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
}

async fn discover(folders: Vec<PathBuf>) -> io::Result<Vec<(String, PathBuf)>> {
    let mut skills = Vec::new(); // At most 1,000 directories; overflow selects nothing.
    let mut seen = BTreeSet::new();
    for folder in folders {
        let mut entries = match fs::read_dir(&folder).await {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        };
        while let Some(entry) = entries.next_entry().await? {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name_valid(&name)
                && fs::try_exists(entry.path().join("SKILL.md"))
                    .await
                    .unwrap_or(false)
                && let Ok(path) = fs::canonicalize(entry.path()).await
            {
                if seen.insert((name.clone(), path.clone())) {
                    skills.push((name, path));
                }
                if skills.len() > 1000 {
                    return Err(refused("more than 1,000 skills"));
                }
            }
        }
    }
    Ok(skills)
}

async fn bounded(path: &Path, cap: u64) -> io::Result<String> {
    let mut text = String::new();
    fs::File::open(path)
        .await?
        .take(cap + 1)
        .read_to_string(&mut text)
        .await?;
    if text.len() as u64 > cap {
        return Err(refused(format!("{} exceeds {cap} bytes", path.display())));
    }
    Ok(text)
}

async fn git(cwd: &Path, args: &[&str]) -> io::Result<String> {
    crate::children::git::timed(cwd, 120, args)
        .await
        .map_err(|e| refused(e.to_string()))
}

async fn tracking(root: &Path, branch: &str) -> io::Result<String> {
    git(
        root,
        &[
            "for-each-ref",
            "--format=%(upstream:remotename) %(upstream)",
            branch,
        ],
    )
    .await
}

async fn checkout(root: &Path) -> io::Result<()> {
    let branch = git(root, &["symbolic-ref", "--quiet", "HEAD"]).await?;
    let before = git(root, &["rev-parse", "HEAD"]).await?;
    let tracked = tracking(root, &branch).await?;
    let (remote, upstream) = tracked
        .split_once(' ')
        .ok_or_else(|| refused("no remote upstream"))?;
    if remote.is_empty() || remote == "." {
        return Err(refused("no remote upstream"));
    }
    clean(root).await?;
    git(root, &["fetch", "--no-recurse-submodules", "--", remote]).await?;
    clean(root).await?;
    if git(root, &["symbolic-ref", "--quiet", "HEAD"]).await? != branch
        || git(root, &["rev-parse", "HEAD"]).await? != before
        || tracking(root, &branch).await? != tracked
    {
        return Err(refused("branch, HEAD or upstream changed during fetch"));
    }
    git(root, &["merge", "--ff-only", "--no-autostash", upstream]).await?;
    let after = git(root, &["rev-parse", "HEAD"]).await?;
    eprintln!(
        "razor: skill updates: {} {before} -> {after}",
        root.display()
    );
    Ok(())
}
async fn clean(root: &Path) -> io::Result<()> {
    if !git(
        root,
        &[
            "status",
            "--porcelain=v1",
            "--untracked-files=all",
            "--ignored",
            "--ignore-submodules=none",
        ],
    )
    .await?
    .is_empty()
    {
        return Err(refused("dirty checkout; left alone"));
    }
    Ok(())
}

async fn starter(name: &str, source: &Path, overrides: &Path) -> io::Result<bool> {
    let upstream = bounded(&source.join("UPSTREAM"), 4096).await?;
    let fields: Vec<_> = upstream.split_whitespace().collect();
    let [repo, reference, folder, bundled] = fields.as_slice() else {
        return Err(refused("malformed UPSTREAM"));
    };
    if repo.starts_with('-')
        || reference.starts_with('-')
        || !Path::new(folder)
            .components()
            .all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
    {
        return Err(refused("invalid UPSTREAM"));
    }
    let moving = git(
        source,
        &[
            "ls-remote",
            "--heads",
            repo,
            &format!("refs/heads/{reference}"),
        ],
    )
    .await?;
    let Some(commit) = moving.split_whitespace().next() else {
        eprintln!(
            "razor: skill updates: starter {name}: no moving branch for {reference}; using pack copy"
        );
        return Ok(false);
    }; // Tag or commit: pinned.
    if !hash_valid(commit) {
        return Err(refused("invalid upstream commit"));
    }
    fs::create_dir_all(overrides).await?;
    let link = overrides.join(name);
    let old = fs::read_link(&link).await.ok();
    let owned = old
        .as_ref()
        .is_some_and(|old| versioned(&old.to_string_lossy()) == Some(name));
    if fs::symlink_metadata(&link).await.is_ok() && !owned {
        return Err(refused("override is not an updater-owned link"));
    }
    let version = format!(".{name}@{commit}");
    if old.as_deref() == Some(Path::new(&version)) || (old.is_none() && commit == *bundled) {
        return Ok(true);
    }
    let commit = install(overrides, name, [repo, reference, folder, commit]).await?;
    let version = format!(".{name}@{commit}");
    let temporary = overrides.join(format!(".{name}.link"));
    if fs::symlink_metadata(&temporary).await.is_ok() {
        fs::remove_file(&temporary).await?;
    }
    fs::symlink(&version, &temporary).await?;
    fs::rename(temporary, &link).await?;
    let mut entries = fs::read_dir(overrides).await?;
    while let Some(entry) = entries.next_entry().await? {
        let file = entry.file_name();
        let file = file.to_string_lossy();
        if versioned(&file) == Some(name)
            && file != version
            && old.as_deref() != Some(Path::new(file.as_ref()))
        {
            fs::remove_dir_all(entry.path()).await?;
        }
    }
    eprintln!("razor: skill updates: starter {name} -> {commit}");
    Ok(true)
}

async fn install(overrides: &Path, name: &str, source: [&str; 4]) -> io::Result<String> {
    let [repo, reference, folder, commit] = source;
    if fs::try_exists(overrides.join(format!(".{name}@{commit}"))).await? {
        return Ok(commit.to_owned());
    }
    let stage = overrides.join(format!(".{name}.stage"));
    if fs::try_exists(&stage).await? {
        fs::remove_dir_all(&stage).await?;
    }
    let work = async {
        git(
            overrides,
            &[
                "-c",
                "core.symlinks=false",
                "clone",
                "--depth=1",
                "--single-branch",
                "--branch",
                reference,
                "--",
                repo,
                &stage.to_string_lossy(),
            ],
        )
        .await?;
        let commit = git(&stage, &["rev-parse", "HEAD"]).await?;
        if !hash_valid(&commit) {
            return Err(refused("invalid upstream commit"));
        }
        let skill = fs::canonicalize(stage.join(folder)).await?;
        if !skill.starts_with(fs::canonicalize(&stage).await?) {
            return Err(refused("upstream skill path leaves its repository"));
        }
        if !fs::try_exists(skill.join("SKILL.md")).await? {
            return Err(refused("upstream skill was deleted"));
        }
        if fs::symlink_metadata(skill.join("UPSTREAM")).await.is_ok() {
            fs::remove_file(skill.join("UPSTREAM")).await?;
        }
        fs::write(
            skill.join("UPSTREAM"),
            format!("{repo} {reference} {folder} {commit}\n"),
        )
        .await?;
        fs::remove_dir_all(stage.join(".git")).await?;
        let destination = overrides.join(format!(".{name}@{commit}"));
        if !fs::try_exists(&destination).await? {
            fs::rename(&skill, &destination).await?;
        }
        Ok(commit)
    }
    .await;
    if fs::try_exists(&stage).await? {
        fs::remove_dir_all(stage).await?;
    }
    work
}

fn hash_valid(hash: &str) -> bool {
    hash.len() == 40 && hash.bytes().all(|c| c.is_ascii_hexdigit())
}

/// The skill whose updater-owned version folder `file` names, `.<name>@<commit>`.
fn versioned(file: &str) -> Option<&str> {
    let (name, hash) = file.strip_prefix('.')?.rsplit_once('@')?;
    (name_valid(name) && hash_valid(hash)).then_some(name)
}

// Only links and version folders bearing the updater's exact naming convention are ours.
async fn retire(overrides: &Path, moving: &BTreeSet<String>) -> io::Result<()> {
    let mut entries = match fs::read_dir(overrides).await {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    while let Some(entry) = entries.next_entry().await? {
        let file = entry.file_name().to_string_lossy().into_owned();
        if let Some(name) = versioned(&file) {
            if !moving.contains(name) {
                fs::remove_dir_all(entry.path()).await?;
            }
        } else if !moving.contains(&file)
            && let Ok(target) = fs::read_link(entry.path()).await
            && versioned(&target.to_string_lossy()) == Some(&file)
        {
            fs::remove_file(entry.path()).await?;
            eprintln!("razor: skill updates: retired starter {file}");
        }
    }
    Ok(())
}
