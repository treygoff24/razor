use super::{Children, Error, TEXT, branch, files, find, other, refuse, remote};
use crate::config;
use crate::controller::Controller;
use crate::protocol::Speed;
use crate::protocol::{
    Body, Child, ChildEvent, ChildRequest, ChildUpdate, Decision, Effort, EntryId, Input, Mail,
    MailKind, Origin, ReportStatus, SessionId, Video,
};
use crate::record::Summary;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::{fs, io::AsyncReadExt};

/// A child's first words, before its shared brief and task: who it is and how its tree works.
const CONTRACT: &str = include_str!("text/child.md");
/// Razor's role texts, each in place of `{role}` unless `[children]` names a file for it.
const REVIEWER: &str = include_str!("text/reviewer.md");
const RESEARCHER: &str = include_str!("text/researcher.md");

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub(super) enum Agent {
    Start(Start),
    StartTop {
        cwd: PathBuf,
        server: Option<String>,
        model: Option<String>,
        effort: Option<Effort>,
        brief: String,
    },
    Continue {
        child: String,
        brief: String,
        model: Option<String>,
        effort: Option<Effort>,
        speed: Option<Speed>,
    },
    Finish {
        status: ReportStatus,
        report: String,
    },
    Settle {
        child: String,
        decision: Decision,
    },
    Stop {
        child: String,
    },
    Wait {
        timeout_ms: Option<u64>,
    },
    Headline {
        text: String,
    },
    Name {
        name: String,
    },
}

#[derive(Default, Deserialize)]
pub(super) struct Start {
    pub(super) name: String,
    pub(super) server: Option<String>,
    pub(super) brief: String,
    pub(super) shared_brief: Option<PathBuf>,
    /// A task of the child's project to claim for it.
    pub(super) task: Option<u32>,
    #[serde(default)]
    pub(super) kind: Kind,
    pub(super) cwd: Option<PathBuf>,
    pub(super) model: Option<String>,
    pub(super) effort: Option<Effort>,
    pub(super) speed: Option<Speed>,
    #[serde(default)]
    pub(super) placement: Placement,
    /// A watcher's video, set by Razor and never by a model: its brief is its whole first input.
    #[serde(skip)]
    pub(super) video: Option<Box<Video>>,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Placement {
    #[default]
    Worktree,
    InPlace,
}

/// What a child is told after its contract; nothing else about a child depends on it.
#[derive(Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Kind {
    #[default]
    Worker,
    Reviewer,
    Researcher,
}

impl Start {
    /// The child's first input: its contract, its role's text (the file `[children]` names, else
    /// Razor's; a worker has none), the shared brief, then its own brief; at most 64 KiB.
    async fn text(&self, parent: &Summary, roles: &config::Children) -> Result<String, Error> {
        let (file, role) = match self.kind {
            Kind::Worker => (None, ""),
            Kind::Reviewer => (roles.reviewer.as_deref(), REVIEWER),
            Kind::Researcher => (roles.researcher.as_deref(), RESEARCHER),
        };
        let role = match file {
            Some(file) => read(file).await?,
            None => role.into(),
        };
        let shared = match &self.shared_brief {
            Some(path) => read(&parent.cwd.join(path)).await?,
            None => String::new(),
        };
        let role = match role.trim_end() {
            "" => String::new(),
            role => format!("{role}\n\n"),
        };
        let text = CONTRACT
            .replace("{name}", &self.name)
            .replace("{parent}", parent.session.as_str())
            .replace("{role}", &role)
            + &shared
            + "\n"
            + &self.brief;
        if text.len() > TEXT {
            return refuse("The role text and briefs are over 64 KiB together; shorten them.");
        }
        Ok(text)
    }
}

/// A file's text, read no further than one byte past what a first input holds.
async fn read(path: &Path) -> Result<String, Error> {
    let mut text = String::new();
    let file = fs::File::open(path).await;
    match async { file?.take(TEXT as u64 + 1).read_to_string(&mut text).await }.await {
        Ok(_) => Ok(text),
        Err(e) => refuse(format!("{} cannot be read: {e}.", path.display())),
    }
}

impl Children {
    pub(super) async fn agent(
        &self,
        controller: &Controller,
        caller: &SessionId,
        at: EntryId,
        action: Agent,
    ) -> Result<(Value, Option<(SessionId, Input)>), Error> {
        let output = match action {
            Agent::StartTop {
                cwd,
                server,
                model,
                effort,
                brief,
            } => {
                let caller_summary = controller.summary(caller).await?;
                if caller_summary.child.is_some() {
                    return refuse("Only a top-level agent can start a top-level session.");
                }
                let mail = Mail {
                    to: String::new(),
                    id: (caller.clone(), at),
                    kind: MailKind::Note,
                    from: super::message::sender(&caller_summary),
                    project: super::project(&caller_summary),
                    text: brief,
                };
                let cwd = caller_summary.cwd.join(cwd);
                let model = model.map(crate::profiles::id);
                if let Some(server) = server {
                    let request = ChildRequest::StartRoot {
                        cwd,
                        model,
                        effort,
                        mail,
                    };
                    let mut result = controller.devices().child(&server, &request).await?;
                    let name = result["name"].as_str().unwrap_or_default();
                    result["address"] = json!(format!("{name}@{server}"));
                    let session = result["session"].as_str().unwrap_or_default();
                    result["session"] = json!(format!("{session}@{server}"));
                    Ok(result)
                } else {
                    self.start_root(controller, cwd, model, effort, mail, None)
                        .await
                }
            }
            Agent::Start(start) => self.start(controller, caller, at, start).await,
            Agent::Continue {
                child,
                brief,
                model,
                effort,
                speed,
            } => {
                let parent = controller.summary(caller).await?;
                let child = find(&parent, &child)?;
                // A child the parent counts as working adds none to the cap.
                if !child.working() {
                    cap(&controller.settings(caller).await?.config.children, &parent)?;
                }
                if brief.len() > TEXT {
                    return refuse("The brief is over 64 KiB; shorten it.");
                }
                let (id, kind, from) = ((caller.clone(), at), MailKind::Note, String::new());
                let next = Input {
                    model: model.map(crate::profiles::id),
                    effort,
                    speed,
                    ..Input::new(Origin::Mail { id, kind, from }, brief)
                };
                if let Some((session, server)) = child.session.address() {
                    // The owner decides from the child's own state; a child this fact counts as
                    // working keeps its task, even one a failed continue never sent it.
                    let task = match child.working() {
                        true => child.task,
                        false => super::parse(
                            controller
                                .update_child(caller, child.session.clone(), ChildUpdate::Continued)
                                .await?["entry"]
                                .clone(),
                        )?,
                    };
                    let input = Input {
                        child_task: Some(task),
                        ..next
                    };
                    controller
                        .devices()
                        .child(server, &ChildRequest::Continue { session, input })
                        .await?;
                } else {
                    controller.continue_child(&child.session, next).await?;
                }
                Ok(json!({}))
            }
            Agent::Finish { status, report } => {
                let summary = controller.summary(caller).await?;
                let (parent, report, files) = super::report(&summary, status, report).await?;
                if let Some(files) = files {
                    controller
                        .append(caller, Body::ChangedFiles { files })
                        .await?;
                }
                return Ok((json!({}), Some((parent, report))));
            }
            Agent::Settle { child, decision } => {
                self.settle(controller, caller, &child, decision).await
            }
            Agent::Stop { child } => stop(controller, caller, &child).await,
            Agent::Wait { timeout_ms } => {
                let timeout = Duration::from_millis(timeout_ms.unwrap_or(600_000));
                let Some(entry) = controller.wait_input(caller, at, timeout).await? else {
                    return Ok((Value::Null, None));
                };
                let entry = controller.get(caller, entry).await?;
                serde_json::from_str(&entry).map_err(other("reading the input it waited for"))
            }
            Agent::Headline { text } => {
                if text.len() > TEXT || text.contains(['\n', '\r']) {
                    return refuse("A headline is one line under 64 KiB.");
                }
                controller.append(caller, Body::Headline { text }).await?;
                Ok(json!({}))
            }
            Agent::Name { name } => super::rename(controller, caller, &name)
                .await
                .map(|()| json!({})),
        }?;
        Ok((output, None))
    }

    pub(super) async fn start(
        &self,
        controller: &Controller,
        caller: &SessionId,
        at: EntryId,
        start: Start,
    ) -> Result<Value, Error> {
        let name = start.name.as_str();
        let word = |b: u8| b.is_ascii_alphanumeric() || b == b'_' || b == b'-';
        if name.is_empty() || name.len() > 64 || !name.bytes().all(word) {
            return refuse("A child's name is 1 to 64 letters, digits, underscores or hyphens.");
        }
        let parent = controller.summary(caller).await?;
        let children = controller.settings(caller).await?.config.children;
        cap(&children, &parent)?;
        let text = match start.video {
            Some(_) => start.brief.clone(),
            None => start.text(&parent, &children).await?,
        };
        let session = SessionId::new();
        let cwd = parent.cwd.join(start.cwd.clone().unwrap_or_default());
        let (worktree, cwd) = match &start.server {
            Some(server) => {
                let root = remote::root(controller, caller).await?;
                if (controller.summary(&root).await?.child)
                    .is_some_and(|c| c.parent.address().is_some())
                {
                    return refuse(
                        "A remote child's subtree starts children on its own server; omit server.",
                    );
                }
                // Validate enrollment before recording an outstanding task.
                if controller.devices().peer(server).await.is_none() {
                    return refuse(format!(
                        "{server} is not paired; pair it before starting a child."
                    ));
                }
                (None, cwd)
            }
            None => {
                let worktree = matches!(start.placement, Placement::Worktree);
                self.place(&session, cwd, worktree).await?
            }
        };
        let held = self.starts.lock().await;
        let taken = remote::names(controller, caller).await?;
        if taken.iter().any(|n| n == name) {
            return refuse(format!(
                "`{name}` is taken in this tree; choose another name."
            ));
        }
        let child = Child {
            parent: caller.clone(),
            name: start.name,
            worktree: worktree.clone(),
            launched: at,
        };
        // Unnamed, they are the parent's own, never the config's defaults.
        let model = start.model.map_or(parent.model, crate::profiles::id);
        let effort = start.effort.unwrap_or(parent.settings.effort);
        let speed = start.speed.unwrap_or_default();
        let (id, kind, from) = ((caller.clone(), at), MailKind::Note, String::new());
        let mut input = Input::new(Origin::Mail { id, kind, from }, text);
        input.video = start.video;
        // A local child's record comes first; the parent's `Started` is the task its reports name.
        let recorded = match &start.server {
            Some(server) => session.on(server),
            None => {
                let (id, choice) = (Some(session.clone()), (Some(effort), Some(speed)));
                let child = Some(child.clone());
                let model = Some(model.clone());
                controller
                    .create(cwd.clone(), model, choice, id, child, None)
                    .await?;
                session.clone()
            }
        };
        let event = ChildEvent::Started(child.clone());
        let started = Body::Child {
            session: recorded.clone(),
            event,
        };
        input.child_task = Some(controller.append(caller, started).await?);
        drop(held);
        let stopped = Body::Child {
            session: recorded.clone(),
            event: ChildEvent::Stopped,
        };
        if let Some(server) = &start.server {
            let request = ChildRequest::Start {
                session,
                child,
                cwd,
                model,
                effort,
                speed,
                in_place: matches!(start.placement, Placement::InPlace),
                task: start.task,
                input,
            };
            let result = controller.devices().child(server, &request).await;
            return match result {
                Ok(mut value) => {
                    value["session"] = json!(recorded);
                    Ok(value)
                }
                Err(e) => {
                    controller.append(caller, stopped).await?;
                    refuse(format!(
                        "Starting child {recorded} failed; inspect its session and use agent stop before retrying: {e}"
                    ))
                }
            };
        }
        if let Some(worktree) = &worktree
            && let Err(e) = self.checkout(&session, worktree).await
        {
            controller.append(caller, stopped).await?;
            return Err(e);
        }
        let task = controller
            .memory()
            .start(controller, &cwd, &session, &child, start.task)
            .await;
        controller.admit(&session, input).await?;
        let branch = worktree.map(|_| branch(&session));
        let mut answer = json!({ "session": session, "cwd": cwd, "branch": branch });
        if let Some(task) = task {
            answer["task"] = task;
        }
        Ok(answer)
    }

    /// Settles every remaining helper as discard when a session ends for good: the person's
    /// close of a root (`/done` after its closing turn, `/quit`), after its final turn releases its
    /// task. An idle close reopens the same session and a helper's finish leaves it continuable, so
    /// neither calls this; a helper's settlement by its parent reaches its own helpers through
    /// `close_children`.
    pub(crate) fn closed(&self, c: &Controller, parent: &Summary) {
        let (c, parent) = (c.clone(), parent.clone());
        tokio::spawn(async move {
            if let Err(e) = c.children().close_children(&c, &parent).await {
                eprintln!("razor: settling helpers of {}: {e}", parent.session);
            }
        });
    }

    /// Settles a session's direct helpers as discard, each settlement cascading to its own, before
    /// the session's checkout goes, since a descendant's worktree may use it as its git origin.
    pub(super) async fn close_children(
        &self,
        c: &Controller,
        parent: &Summary,
    ) -> Result<(), Error> {
        let mut failure = None;
        for child in parent
            .children
            .iter()
            .filter(|child| child.settled.is_none())
        {
            let result = async {
                if child.working() {
                    stop(c, &parent.session, &child.child.name).await?;
                }
                self.settle(c, &parent.session, &child.child.name, Decision::Discard)
                    .await
            }
            .await;
            if let Err(e) = result {
                eprintln!("razor: settling helper {}: {e}", child.session);
                failure = Some(e);
            }
        }
        failure.map_or(Ok(()), Err)
    }

    async fn settle(
        &self,
        controller: &Controller,
        caller: &SessionId,
        name: &str,
        decision: Decision,
    ) -> Result<Value, Error> {
        let parent = controller.summary(caller).await?;
        let child = find(&parent, name)?;
        if child.settled.is_some() || (child.report.is_none() && !child.stopped) {
            return refuse("Settle a finished or stopped child once; stop a working child first.");
        }
        let memory = controller.memory();
        let report = memory
            .settled(controller, &parent, &child, decision)
            .await
            .map_err(|e| Error::Refused(e.to_string()))?;
        let text = if let Some((session, server)) = child.session.address() {
            controller
                .devices()
                .child(
                    server,
                    &ChildRequest::Settle {
                        session,
                        decision,
                        report: report.clone(),
                    },
                )
                .await?["text"]
                .as_str()
                .unwrap_or_default()
                .to_owned()
        } else {
            self.settle_git(controller, &child.session, &child.child, decision, None)
                .await?
        };
        memory
            .released(controller, &parent, &child, report.as_ref())
            .await;
        let (session, event) = (child.session, ChildEvent::Settled(decision));
        controller
            .append(caller, Body::Child { session, event })
            .await?;
        Ok(json!({ "text": text }))
    }
}

/// The explicit stop and close-time cleanup share the recorded stop before settlement.
async fn stop(c: &Controller, caller: &SessionId, name: &str) -> Result<Value, Error> {
    let child = find(&c.summary(caller).await?, name)?;
    c.close(&child.session).await?;
    if child.session.address().is_none() {
        files(c, &child.session).await?;
    }
    let (session, event) = (child.session, ChildEvent::Stopped);
    c.append(caller, Body::Child { session, event }).await?;
    Ok(json!({}))
}

/// Refuses one more working child once `children.max` of the parent's are at work.
fn cap(children: &config::Children, parent: &Summary) -> Result<(), Error> {
    let max = children.max;
    let working = parent.children.iter().filter(|c| c.working()).count();
    if working >= max {
        return refuse(format!(
            "This session has {working} working children and children.max is {max}; raise it live or wait for one to finish."
        ));
    }
    Ok(())
}
