//! Children are ordinary sessions. Their lifecycle is facts in the parent's record, mail is
//! input to the recipient, and a tree shares one channel file (docs/design/children.md).

mod agent;
pub(crate) mod git;
mod message;
mod post;
mod remote;
mod search;
pub(crate) use remote::{
    child, child_row, continuation, deliver, descendants, records, recover, recover_loaded, root,
    update,
};
pub(crate) use search::{digest, search};
mod turn;
mod watch;
pub(crate) use message::{name, path_identity, project, receive, repo, repo_info, sender};
pub(crate) use post::Post;
pub(crate) use turn::Turn;

use crate::controller::{self, Controller};
use crate::protocol::{
    Body, EntryId, Input, Origin, ReportStatus, SessionId, Spoken, State, Worktree,
};
use crate::record::{ChildSummary, Summary};
use futures_util::future::{BoxFuture, join_all};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::{LazyLock, OnceLock};
use std::{fmt, path::PathBuf};
use tokio::{fs, sync::Mutex};

/// The most bytes in a first input, a continuation, a headline or an encoded channel post.
const TEXT: usize = 64 * 1024;

static WORDS: LazyLock<BTreeMap<String, String>> = LazyLock::new(|| {
    // Invariant: the file is compiled into the binary, so it parses in every run of a build or
    // in none, and the first Post ring of any flow shows which.
    toml::from_str(include_str!("text/mail.toml")).expect("src/children/text/mail.toml parses")
});

/// One controller's child, mail and channel actions. It keeps no session state: every fact
/// it acts on is read from the controller when it acts.
pub(crate) struct Children {
    home: PathBuf,
    /// Held from a start's name check to its record, so two parents cannot take one name.
    starts: Mutex<()>,
    /// Held from a watcher's name to its start, through its video's admission.
    watching: Mutex<()>,
    /// Post, when installed: each session's participant and watch.
    pub(crate) post: Post,
    /// The API server's voice note maker, set once as it starts serving.
    speak: OnceLock<Speak>,
}

/// Makes a root's note at its call's entry spoken, given its text and whether the person asked
/// for it: its voice and sound, or a refusal's sentence.
type Speak = Box<
    dyn Fn(SessionId, EntryId, String, bool) -> BoxFuture<'static, Result<Spoken, Error>>
        + Send
        + Sync,
>;

/// A sentence the model can act on, or a machine failure with what was being done.
#[derive(Debug)]
pub(crate) enum Error {
    Refused(String),
    Missing(SessionId),
    Other {
        source: Box<dyn std::error::Error + Send + Sync>,
        context: &'static str,
    },
}

impl Children {
    /// Scans no records and opens no file; one serves the whole controller.
    pub(crate) fn open(home: PathBuf) -> Self {
        Self {
            post: Post::open(home.clone()),
            home,
            starts: Mutex::new(()),
            watching: Mutex::new(()),
            speak: OnceLock::new(),
        }
    }

    /// Makes the `message` action's voice notes through `speak`; a second call does nothing.
    pub(crate) fn speak(&self, speak: Speak) {
        let _ = self.speak.set(speak);
    }

    /// Runs `caller`'s recorded call `at`: `agent`, `message`, `recall` or `status`. Beside the output,
    /// a finished child's report waits for the caller's result, usage and close before admission.
    pub(crate) async fn run(
        &self,
        controller: &Controller,
        caller: &SessionId,
        at: EntryId,
        action: &str,
        input: Value,
    ) -> Result<(Value, Option<(SessionId, Input)>), Error> {
        let output = match action {
            "agent" => return self.agent(controller, caller, at, parse(input)?).await,
            "message" => self.message(controller, caller, at, parse(input)?).await?,
            "recall" => search::recall(controller, input).await?,
            // The controller sends children nothing but agent, message, recall and status.
            _ => status(self, controller, caller).await?,
        };
        Ok((output, None))
    }

    fn dir(&self, session: &SessionId) -> PathBuf {
        self.home.join("sessions").join(session.as_str())
    }

    /// Where a child starting in `cwd` works: with `worktree` in a checkout, the same place in
    /// its own worktree of that checkout, and the checkout; else `cwd` itself, made absolute.
    async fn place(
        &self,
        session: &SessionId,
        cwd: PathBuf,
        worktree: bool,
    ) -> Result<(Option<Worktree>, PathBuf), Error> {
        let cwd = fs::canonicalize(cwd)
            .await
            .map_err(other("opening the child's folder"))?;
        let found = if worktree {
            git::plan(&cwd).await?
        } else {
            None
        };
        Ok(match found {
            Some((worktree, inside)) => {
                let cwd = self.dir(session).join("worktree").join(inside);
                (Some(worktree), cwd)
            }
            None => (None, cwd),
        })
    }

    /// Checks out the child's own branch at its worktree's base, in its session's folder.
    async fn checkout(&self, session: &SessionId, worktree: &Worktree) -> Result<(), Error> {
        git::add(
            worktree,
            &self.dir(session).join("worktree"),
            &branch(session),
        )
        .await
    }
}

/// Every agent in the caller's tree but settled helpers; local working children have current
/// changed files, remote helpers their owner's recorded paths or null when it cannot answer.
async fn status(
    children: &Children,
    controller: &Controller,
    caller: &SessionId,
) -> Result<Value, Error> {
    let rows = children
        .rows(controller, controller.tree(caller).await?)
        .await;
    let remote = join_all(rows.iter().map(|row| async move {
        if let Some((id, server)) = row.session.address()
            && let Some(device) = controller.devices().peer(server).await
        {
            controller
                .devices()
                .fetch::<crate::protocol::AgentDetail>(
                    &device,
                    &format!("/v1/sessions/{id}/agent"),
                    &(),
                    crate::protocol::BODY_BYTES,
                )
                .await
                .and_then(|detail| detail.changed_files)
        } else {
            None
        }
    }))
    .await;
    let mut agents = Vec::new();
    for (mut row, mut changed_files) in rows.into_iter().zip(remote) {
        if row.session.address().is_none() {
            changed_files = Some(controller.summary(&row.session).await?.changed_files);
        } else {
            controller.parent_row(&mut row).await?;
        }
        if row.state != State::Done
            && row.session.address().is_none()
            && let Some(files) = files(controller, &row.session).await?
        {
            changed_files = Some(files);
        }
        agents.push(json!({"session": row.session, "parent": row.parent, "name": row.name, "headline": row.headline,
            "state": row.state, "changed_files": changed_files, "branch": row.branch}));
    }
    Ok(json!({ "agents": agents }))
}

/// Records and returns the files a child's branch changed since its base when git can list
/// them and they differ from the last list; otherwise the last list stands.
async fn files(c: &Controller, session: &SessionId) -> Result<Option<Vec<PathBuf>>, Error> {
    let files = changed(&c.summary(session).await?).await;
    if let Some(files) = files.clone() {
        c.append(session, Body::ChangedFiles { files }).await?;
    }
    Ok(files)
}

async fn changed(summary: &Summary) -> Option<Vec<PathBuf>> {
    let worktree = summary.child.as_ref()?.worktree.as_ref()?;
    let files = git::files(worktree, &branch(&summary.session)).await?;
    (files != summary.changed_files).then_some(files)
}

/// Prepares the same parent, bounded report and changed files for explicit and implicit finish,
/// without re-entering the session task that is preparing its own report.
pub(crate) async fn report(
    summary: &Summary,
    status: ReportStatus,
    text: String,
) -> Result<(SessionId, Input, Option<Vec<PathBuf>>), Error> {
    if text.len() > TEXT {
        return refuse("The report is over 64 KiB; shorten it.");
    }
    let Some(child) = &summary.child else {
        return refuse("Only a child finishes; answer the person in your reply.");
    };
    let no_changes = match &child.worktree {
        None => true,
        Some(w) => git::timed(&w.origin, 10, &["rev-parse", &branch(&summary.session)])
            .await
            .is_ok_and(|tip| tip == w.base),
    };
    let from = Origin::Report {
        child: summary.session.clone(),
        status,
    };
    Ok((
        child.parent.clone(),
        Input {
            child_task: summary.child_task,
            child_no_changes: no_changes,
            ..Input::new(from, text)
        },
        changed(summary).await,
    ))
}

/// Renames a top-level session, for the person's `PUT /v1/sessions/{id}/name` and the `agent`
/// tool's `name` action: one line of 1 to 64 characters. A child's name is its parent's to give.
pub(crate) async fn rename(
    controller: &Controller,
    session: &SessionId,
    name: &str,
) -> Result<(), Error> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 64 || name.contains(['\n', '\r']) {
        return refuse("A name is one line of 1 to 64 characters.");
    }
    if controller.summary(session).await?.child.is_some() {
        return refuse("A child's name is its parent's to give; only a top-level session renames.");
    }
    let name = name.into();
    controller.append(session, Body::Name { name }).await?;
    Ok(())
}

/// The caller's direct child, by name or session id.
fn find(parent: &Summary, name: &str) -> Result<ChildSummary, Error> {
    let named = |c: &&ChildSummary| c.child.name == name || c.session.as_str() == name;
    match parent.children.iter().find(named) {
        Some(child) => Ok(child.clone()),
        None => refuse(format!(
            "This session has no child `{name}`; status lists them."
        )),
    }
}

/// One of `text/mail.toml`'s texts.
fn words(key: &str) -> &'static str {
    WORDS.get(key).map_or("", String::as_str)
}

/// The sole branch name for a child's existing session, shared with its status row.
pub(crate) fn branch(session: &SessionId) -> String {
    format!("child/{session}")
}

fn refuse<T>(sentence: impl Into<String>) -> Result<T, Error> {
    Err(Error::Refused(sentence.into()))
}

fn parse<T: serde::de::DeserializeOwned>(input: Value) -> Result<T, Error> {
    serde_json::from_value(input).or_else(|e| refuse(format!("The input does not fit: {e}.")))
}

fn other<E>(context: &'static str) -> impl FnOnce(E) -> Error
where
    E: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    move |e| Error::Other {
        source: e.into(),
        context,
    }
}

impl From<controller::Error> for Error {
    fn from(e: controller::Error) -> Self {
        match e {
            controller::Error::Invalid(sentence) => Error::Refused(sentence),
            controller::Error::NoSession(session) => Error::Missing(session),
            e => other("asking the controller")(e),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Refused(sentence) => f.write_str(sentence),
            Error::Missing(session) => write!(f, "There is no session {session}."),
            Error::Other { source, context } => write!(f, "Razor failed {context}: {source}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<crate::devices::Error> for Error {
    fn from(e: crate::devices::Error) -> Self {
        match e {
            crate::devices::Error::Refused(sentence) => Error::Refused(sentence),
            e => other("reaching a paired server")(e),
        }
    }
}
