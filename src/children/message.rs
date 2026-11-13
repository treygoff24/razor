use super::{Children, Error, TEXT, find, other, refuse};
use crate::controller::Controller;
use crate::protocol::{
    AgentView, Ask, Body, ChannelEntry, EntryId, Input, Live, Mail, MailId, MailKind, Origin,
    SessionId, State, now,
};
use crate::record::Summary;
use crate::residents;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::Path;
use std::{cmp::Reverse, io::SeekFrom, path::PathBuf, sync::LazyLock, sync::Mutex};
use tokio::{
    fs,
    io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt},
};

/// The most agents `list` shows from one server.
const LISTED: usize = 100;

#[derive(Deserialize)]
pub(super) struct Message {
    r#type: String,
    to: Option<String>,
    #[serde(default)]
    text: String,
    question: Option<MailId>,
    #[serde(default)]
    options: Vec<String>,
    limit: Option<usize>,
    query: Option<String>,
    #[serde(default)]
    voice: bool,
    #[serde(default)]
    requested: bool,
}

impl Children {
    pub(super) async fn message(
        &self,
        controller: &Controller,
        caller: &SessionId,
        at: EntryId,
        input: Message,
    ) -> Result<Value, Error> {
        let kind = match (input.r#type.as_str(), input.question) {
            ("read", _) => return self.read(controller, caller, input.limit).await,
            ("list", _) => return self.list(controller, caller, input.query).await,
            ("note", _) => MailKind::Note,
            ("question", _) => MailKind::Question,
            ("answer", Some(question)) => MailKind::Answer { question },
            _ => {
                return refuse(
                    "Send a note, a question, an answer with its question id; or read, or list.",
                );
            }
        };
        let id = (caller.clone(), at);
        let summary = controller.summary(caller).await?;
        let spoken = if input.voice {
            if input.r#type != "note" || !matches!(input.to.as_deref(), None | Some("parent")) {
                return refuse(
                    "Only a root's note to the person may be spoken; write to your parent instead.",
                );
            }
            let Some(speak) = self.speak.get() else {
                return refuse("Voice notes start with the server; write instead.");
            };
            let note = speak(caller.clone(), at, input.text.clone(), input.requested);
            Some(note.await?)
        } else {
            None
        };
        let from = sender(&summary);
        let foreign_parent = summary
            .child
            .as_ref()
            .filter(|c| c.parent.address().is_some())
            .map(|c| c.parent.to_string());
        let to = match input.to.as_deref() {
            None | Some("parent") if foreign_parent.is_some() => foreign_parent.as_deref(),
            to => to,
        };
        let to = match to {
            Some("channel") if kind == MailKind::Note => {
                return self.post(controller, caller, from, id, input.text).await;
            }
            Some("channel") => return refuse("The channel takes notes; ask or answer the agent."),
            None | Some("parent") => summary.child.as_ref().map(|c| c.parent.clone()),
            Some(to) => match find(&summary, to).ok() {
                // Mail to an ended child would wait in its record forever, holding it live.
                Some(c) if c.report.is_some() || c.stopped || c.settled.is_some() => {
                    return refuse(format!(
                        "`{to}` has ended; continue it with agent to give it more work."
                    ));
                }
                Some(c) if c.session.address().is_none() => Some(c.session),
                // A foreign child and every agent that is not a child share the mail route.
                found => {
                    let to = found.map_or_else(|| to.to_string(), |c| c.session.to_string());
                    let text = preview(&input.text);
                    let mail = Mail {
                        to,
                        id: id.clone(),
                        kind: kind.clone(),
                        from,
                        project: project(&summary),
                        text: input.text,
                    };
                    let to = self.send(controller, mail).await?;
                    let sent = Body::MailSent {
                        id: id.clone(),
                        to: to.clone(),
                        kind,
                        text,
                    };
                    controller.append(caller, sent).await?;
                    return Ok(json!({ "id": id, "to": to }));
                }
            },
        };
        // A root's parent is the person: a note goes to the inbox and the notify command, and a
        // question is an ask.
        match (to, kind) {
            (Some(to), kind) => {
                mail(controller, caller, &to, id.clone(), kind, from, input.text).await?
            }
            // It is their lock screen's notification; a spoken one is checked as a voice note.
            (None, MailKind::Note)
                if spoken.is_none()
                    && (input.text.chars().count() > 300 || input.text.contains(['\r', '\n'])) =>
            {
                return refuse("Shorten this note to one line of at most 300 characters.");
            }
            (None, MailKind::Note) => {
                let note = controller
                    .deliveries()
                    .note(controller, caller, at, &input.text, spoken)
                    .await;
                note.map_err(other("keeping the note"))?;
                let config = controller.config().await;
                residents::notify(&config, input.text, residents::environment(&summary));
            }
            (None, MailKind::Question) => {
                let config = controller.config().await;
                let (text, options) = (input.text, input.options);
                let ask = Ask {
                    id: id.clone(),
                    text,
                    options,
                    at: now(),
                    waited: false,
                };
                residents::ask(controller, &config, &ask, &summary);
            }
            (None, _) => {
                return refuse("The person asked nothing; an answer goes to the child that asked.");
            }
        }
        Ok(json!({ "id": id }))
    }

    /// Appends a note to the tree's channel, then mails it to each agent it mentions.
    async fn post(
        &self,
        controller: &Controller,
        caller: &SessionId,
        from: String,
        id: MailId,
        text: String,
    ) -> Result<Value, Error> {
        let entry = ChannelEntry {
            id: id.clone(),
            at: now(),
            text: text.clone(),
        };
        let line = json!(entry).to_string() + "\n";
        if line.len() > TEXT {
            return refuse("This post is over 64 KiB once encoded; shorten it.");
        }
        // With O_APPEND each post is one write at the file's end; posts from two sessions never mix.
        let (_, mut file) = self.channel(controller, caller).await?;
        let written = async {
            file.write_all(line.as_bytes()).await?;
            file.flush().await
        };
        written.await.map_err(other("posting to the channel"))?;
        for row in controller.tree(caller).await? {
            if row.session.address().is_none()
                && row.session != *caller
                && row.report.is_none()
                && mentions(&text, &row)
            {
                let (id, from, text) = (id.clone(), from.clone(), text.clone());
                mail(
                    controller,
                    caller,
                    &row.session,
                    id,
                    MailKind::Mention,
                    from,
                    text,
                )
                .await?;
            }
        }
        Ok(json!({ "id": id }))
    }

    /// The latest posts (20 unless `limit` says) in the channel's last 64 KiB, and the path of
    /// the channel's file, which holds every post.
    async fn read(
        &self,
        controller: &Controller,
        caller: &SessionId,
        limit: Option<usize>,
    ) -> Result<Value, Error> {
        let (path, mut file) = self.channel(controller, caller).await?;
        let mut bytes = Vec::new();
        let read = async {
            let start = file.metadata().await?.len().saturating_sub(TEXT as u64);
            file.seek(SeekFrom::Start(start)).await?;
            file.take(TEXT as u64).read_to_end(&mut bytes).await?;
            Ok::<_, std::io::Error>(start)
        };
        let start = read.await.map_err(other("reading the channel"))?;
        // A window that starts mid-file may cut its first line; a damaged line is skipped.
        let lines = bytes.split(|b| *b == b'\n').skip(usize::from(start > 0));
        let mut posts: Vec<ChannelEntry> = lines
            .filter_map(|line| serde_json::from_slice(line).ok())
            .collect();
        posts.drain(..posts.len().saturating_sub(limit.unwrap_or(20)));
        Ok(json!({ "posts": posts, "file": path }))
    }

    /// The tree's channel file, made on first use, open to read and append.
    async fn channel(
        &self,
        controller: &Controller,
        caller: &SessionId,
    ) -> Result<(PathBuf, fs::File), Error> {
        let root = super::remote::root(controller, caller).await?;
        let dir = self.home.join("channels");
        let made = fs::create_dir_all(&dir).await;
        made.map_err(other("making the channel directory"))?;
        let path = dir.join(format!("{root}.jsonl"));
        let mut open = fs::OpenOptions::new();
        let file = open.read(true).append(true).create(true).open(&path).await;
        Ok((path, file.map_err(other("opening the channel"))?))
    }

    /// Live agents, residents and the caller's tree, except done agents and the caller, newest activity
    /// first, `tree` marking the caller's own tree; with `query`, only those whose id, name,
    /// project, repository or headline holds it in any case. At most 100 from each server, and
    /// `more` counts the rest; a server that does not answer in 2 seconds is named in `silent`.
    async fn list(
        &self,
        controller: &Controller,
        caller: &SessionId,
        query: Option<String>,
    ) -> Result<Value, Error> {
        let tree = controller.tree(caller).await?;
        let query = query.unwrap_or_default().to_lowercase();
        let (mut agents, mut more) = (Vec::new(), 0);
        let mut add = |rows: Vec<AgentView>, at: &str| {
            let holds = |r: &&AgentView| {
                let repo = r.repo.as_deref().unwrap_or_default();
                let fields = [r.session.as_str(), &r.name, &r.project, repo, &r.headline];
                fields.iter().any(|f| f.to_lowercase().contains(&query))
            };
            let rows = listed(rows, caller);
            let found: Vec<&AgentView> = rows.iter().filter(holds).collect();
            more += found.len().saturating_sub(LISTED);
            for r in found.into_iter().take(LISTED) {
                let to = format!("{}{at}", r.session);
                let tree = tree.iter().any(|t| t.session.as_str() == to);
                agents.push(json!({"to": to, "name": r.name, "project": r.project,
                    "state": r.state, "headline": r.headline, "tree": tree}));
            }
        };
        let mut local = snapshot_here(controller).await;
        for row in &tree {
            if row.session.address().is_none() && !local.iter().any(|r| r.session == row.session) {
                local.push(row.clone());
            }
        }
        add(local, "");
        let servers = controller.devices().paired().await;
        let asked = servers
            .iter()
            .map(|(device, _)| controller.devices().snapshot(device));
        let (mut silent, answers) = (Vec::new(), futures_util::future::join_all(asked).await);
        for ((device, _), rows) in servers.iter().zip(answers) {
            match rows {
                Some(rows) => add(rows, &format!("@{}", device.name)),
                None => silent.push(device.name.clone()),
            }
        }
        Ok(json!({ "agents": agents, "more": more, "silent": silent }))
    }

    /// Admits outside mail here, or posts it to the paired server its `to` names after `@`;
    /// returns the session that took it, with `@<server>` when remote.
    async fn send(&self, controller: &Controller, mut mail: Mail) -> Result<String, Error> {
        let Some((who, server)) = mail
            .to
            .rsplit_once('@')
            .map(|(w, s)| (w.into(), s.to_string()))
        else {
            return Ok(receive(controller, mail, None).await?.0.to_string());
        };
        mail.to = who;
        Ok(controller.devices().mail(&server, &mail).await?)
    }
}

/// Admits `mail` to the one agent on this server that `mail.to` names, and returns that agent
/// and the input's entry. `server` is the paired device the mail came through, and the mail is
/// from `<session>@<server>`; None from here.
/// - Exact local ids also resolve from their records. Names and projects resolve among current
///   rows and the caller's tree, excluding done agents and the sender. Only when no name matches,
///   project of each session that is no child.
/// - No candidate, or more than one: refused with one sentence naming up to ten by id;
///   nothing is admitted.
pub(crate) async fn receive(
    controller: &Controller,
    mail: Mail,
    server: Option<&str>,
) -> Result<(SessionId, EntryId), Error> {
    let to = mail.to.as_str();
    let mut rows = snapshot_here(controller).await;
    if let Ok(id) = SessionId::try_from(to.to_owned())
        && id.address().is_none()
        && !rows.iter().any(|r| r.session == id)
        && let Ok(row) = controller.agent(&id).await
    {
        rows.push(row);
    }
    if server.is_none() && !rows.iter().any(|r| r.session.as_str() == to) {
        for row in controller.tree(&mail.id.0).await.unwrap_or_default() {
            if row.session.address().is_none() && !rows.iter().any(|r| r.session == row.session) {
                rows.push(row);
            }
        }
    }
    let rows = listed(rows, &mail.id.0);
    let exact = |r: &&AgentView| r.session.as_str() == to || (!to.is_empty() && r.name == to);
    let mut found: Vec<&AgentView> = rows.iter().filter(exact).collect();
    if found.is_empty() {
        let project = |r: &&AgentView| r.parent.is_none() && r.project == to;
        found = rows.iter().filter(project).collect();
    }
    let session = match found.as_slice() {
        [row] => row.session.clone(),
        [] => {
            return refuse(format!(
                "No agent is `{to}`, or it has ended; message list shows who is."
            ));
        }
        rows => {
            let named = rows
                .iter()
                .take(10)
                .map(|r| format!("{} ({} in {})", r.session, r.name, r.project));
            return refuse(format!(
                "`{to}` names {} agents: {}; send to one by its id.",
                rows.len(),
                named.collect::<Vec<_>>().join(", ")
            ));
        }
    };
    let (mut id, mut kind) = (mail.id, mail.kind);
    let from = match server {
        Some(server) => {
            // The paired server's session is named by the key it came with, never by its words;
            // no local id has an `@`, so it is never taken for a parent or a child here. An
            // answer's question is a session here, read there with an `@` that it loses here.
            let qualified = SessionId::try_from(format!("{}@{server}", id.0));
            id.0 = qualified.map_err(other("naming a paired server's session"))?;
            if let MailKind::Answer { question } = &mut kind
                && let Some((asker, _)) = question.0.as_str().split_once('@')
            {
                let asker = SessionId::try_from(asker.to_string());
                question.0 = asker.map_err(other("reading an answer's question"))?;
            }
            format!("{}@{server} in {}", mail.from, mail.project)
        }
        None => format!("{} in {}", mail.from, mail.project),
    };
    let entry = controller
        .admit(
            &session,
            Input::new(Origin::Mail { id, kind, from }, mail.text),
        )
        .await?;
    Ok((session, entry))
}

/// The name Razor gives a root: `<first>-<second>` (a server's name and its folder's last name,
/// or a resident's and a word), each run of characters a mention splits a word on made one `-`;
/// then the first of it, `-2`, `-3` … not in `taken`. Empty when both parts are, so the record's
/// fallback names the root.
pub(crate) fn name(first: &str, second: &str, taken: &[&str]) -> String {
    let words = format!("{first} {second}");
    let words = words.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'));
    let base = words.filter(|w| !w.is_empty()).collect::<Vec<_>>();
    let base = base.join("-");
    let mut name = base.clone();
    for n in 2.. {
        if !taken.contains(&name.as_str()) {
            break;
        }
        name = format!("{base}-{n}");
    }
    name
}

/// The name `s` signs its mail with: its own, or while it has none its id, which the reader
/// can answer.
pub(crate) fn sender(s: &Summary) -> String {
    match s.name.as_str() {
        "" => s.session.to_string(),
        name => name.to_owned(),
    }
}

/// The last name of the folder `s` started in, its origin checkout's for a worktree child.
pub(crate) fn project(s: &Summary) -> String {
    let worktree = s.child.as_ref().and_then(|c| c.worktree.as_ref());
    let folder = worktree.map_or(&s.cwd, |w| &w.origin);
    folder
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
}

/// The repository the session's folder belongs to, named by its main checkout's folder, so a
/// worktree folds under it; None outside git. Read once per folder from the git files, never by
/// running git.
pub(crate) fn repo(s: &Summary) -> Option<String> {
    static REPOS: LazyLock<Mutex<HashMap<PathBuf, Option<String>>>> =
        LazyLock::new(Default::default);
    let worktree = s.child.as_ref().and_then(|c| c.worktree.as_ref());
    let folder = worktree.map_or(&s.cwd, |w| &w.origin);
    let mut repos = REPOS.lock().unwrap_or_else(|e| e.into_inner());
    repos
        .entry(folder.clone())
        .or_insert_with(|| repo_info(folder).map(|(name, _)| name))
        .clone()
}

/// A checkout's display name, its main checkout's folder name, and its cross-server identity,
/// from its origin remote, else its path.
pub(crate) fn repo_info(folder: &Path) -> Option<(String, String)> {
    let top = folder.ancestors().find(|d| d.join(".git").exists())?;
    let (mut common, mut main) = (top.join(".git"), top.canonicalize().ok()?);
    if let Ok(link) = std::fs::read_to_string(&common) {
        common = top.join(link.strip_prefix("gitdir:")?.trim());
        // A linked worktree is named by its main checkout, or its bare repository less `.git`;
        // one of a submodule or of a separate git folder, both rare, by that git folder.
        if let Ok(shared) = std::fs::read_to_string(common.join("commondir")) {
            common = common.join(shared.trim()).canonicalize().ok()?;
            main.clone_from(&common);
            if main.ends_with(".git") {
                main.pop();
            } else if main.extension().is_some_and(|e| e == "git") {
                main.set_extension("");
            }
        }
    }
    let config = std::fs::read_to_string(common.join("config")).unwrap_or_default();
    let origin = config
        .split("\n[")
        .find(|s| s.trim_start_matches('[').starts_with("remote \"origin\"]"));
    let url = origin.and_then(|s| s.lines().find_map(|l| l.trim().strip_prefix("url")));
    let url = url.map(|u| {
        u.trim_start_matches([' ', '='])
            .trim()
            .trim_end_matches('/')
    });
    // A checkout at the filesystem root is named by its path.
    let name = main.file_name().unwrap_or(main.as_os_str());
    let name = name.to_string_lossy().into_owned();
    let identity = match url {
        Some(url) => {
            let (scheme, url) = url
                .split_once("://")
                .map_or((false, url), |(_, rest)| (true, rest));
            let (authority, path) = url.split_once('/').unwrap_or((url, ""));
            let authority = authority
                .rsplit_once('@')
                .map_or(authority, |(_, host)| host);
            let url = if scheme {
                format!("{authority}/{path}")
            } else {
                format!("{}/{}", authority.replacen(':', "/", 1), path)
            }
            .to_lowercase();
            format!(
                "remote:{}",
                url.trim_end_matches('/').trim_end_matches(".git")
            )
        }
        None => path_identity(top),
    };
    Some((name, identity))
}

pub(crate) fn path_identity(path: &Path) -> String {
    let relative = std::env::var_os("HOME")
        .and_then(|home| path.strip_prefix(home).ok())
        .unwrap_or(path);
    format!("path:{}", relative.display())
}

/// Admits `text` to `to` as mail, then records in the sender's record that it was sent.
async fn mail(
    controller: &Controller,
    caller: &SessionId,
    to: &SessionId,
    id: MailId,
    kind: MailKind,
    from: String,
    text: String,
) -> Result<(), Error> {
    let origin = Origin::Mail {
        id: id.clone(),
        kind: kind.clone(),
        from,
    };
    let sent = Body::MailSent {
        id,
        to: to.to_string(),
        kind,
        text: preview(&text),
    };
    controller.admit(to, Input::new(origin, text)).await?;
    controller.append(caller, sent).await?;
    Ok(())
}

/// The first line, capped by Unicode characters rather than UTF-8 bytes.
fn preview(text: &str) -> String {
    let line = text.lines().next().unwrap_or_default();
    line.chars().take(200).collect()
}

/// Whether `text` has the word `@name` or `@session` for this agent.
fn mentions(text: &str, row: &AgentView) -> bool {
    text.split(|c: char| !(c.is_ascii_alphanumeric() || "@_-".contains(c)))
        .filter_map(|word| word.strip_prefix('@'))
        .any(|word| word == row.name || word == row.session.as_str())
}

/// This server's status rows.
async fn snapshot_here(controller: &Controller) -> Vec<AgentView> {
    match controller.status().recv().await {
        Some(Live::Agents { agents, .. }) => agents,
        _ => Vec::new(),
    }
}

/// `rows` not done and not `me`, newest activity first.
fn listed(mut rows: Vec<AgentView>, me: &SessionId) -> Vec<AgentView> {
    rows.retain(|r| r.state != State::Done && r.session != *me);
    rows.sort_by_key(|r| Reverse(r.last_activity));
    rows
}
