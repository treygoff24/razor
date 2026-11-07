//! The notify and ask commands, which carry notifications and asks to the person: detached, at
//! most 16 at a time between them.

use super::{FILE, TEXT};
use crate::config::Config;
use crate::controller::Controller;
use crate::protocol::{Answer, Ask, Body, End, EntryId, Input, Origin, SessionId};
use crate::record::Record;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::sync::{Semaphore, SemaphorePermit};

/// Asks and notifications share at most 16 detached commands.
static RUNNING: Semaphore = Semaphore::const_new(16);

/// Notify for each scheduled input in a committed failed turn or withdrawal.
/// Reads only the named input entries (at most 64); detached notifications share the asks'
/// 16-command bound and `notify`'s 30-second limit; failures are logged, never recorded.
pub(crate) async fn failed(
    config: &Config,
    session: &SessionId,
    record: &Record,
    inputs: &[EntryId],
    end: &End,
) {
    let template = match end {
        End::Done => return,
        End::Error(_) => &TEXT.schedule_failed,
        End::Stopped => &TEXT.schedule_stopped,
    };
    // A fresh run's thread reports as its resident.
    let session = record
        .summary()
        .thread
        .as_ref()
        .map_or(session, |t| &t.resident);
    if !config.residents.contains_key(session) {
        return;
    }
    for id in inputs {
        let entry = match record.entry(*id).await {
            Ok(entry) => entry,
            Err(e) => {
                eprintln!("razor: resident {session}'s failed turn: {e}");
                continue;
            }
        };
        let Body::Input(Input {
            from: Origin::Schedule { name, .. },
            ..
        }) = entry.body
        else {
            continue;
        };
        let mut text = template
            .replace("{name}", session.as_str())
            .replace("{schedule}", &name);
        if let End::Error(why) = end {
            text = text.replace("{why}", why.trim_end_matches('.'));
        }
        notify(config, text);
    }
}

/// Send the person a notification through the notify command, detached, under the bound of 16
/// commands asks share. A failure, a 30-second timeout or a full bound is logged; with no
/// command configured, nothing runs. A note to the person is already in the inbox.
pub(crate) fn notify(config: &Config, text: String) {
    let full = "this one is not sent";
    let Some((mut run, permit)) = detached(&config.notify, &text, full) else {
        return;
    };
    tokio::spawn(async move {
        let _permit = permit;
        match tokio::time::timeout(Duration::from_secs(30), run.status()).await {
            Ok(Ok(status)) if status.success() => {}
            Ok(Ok(status)) => eprintln!("razor: The notify command failed ({status}): {text}"),
            Ok(Err(e)) => eprintln!("razor: The notify command did not run ({e}): {text}"),
            Err(_) => eprintln!("razor: The notify command did not exit in 30 seconds: {text}"),
        }
    });
}

/// Carry an ask to the person through the ask command, and return at once. The command runs
/// detached, at most 16 at a time; when it exits 0 and prints an answer, Razor answers the ask
/// with `Controller::answer`, which refuses an ask already answered from a client.
pub(crate) fn ask(controller: &Controller, config: &Config, ask: &Ask) {
    let mut question = ask.text.clone();
    for (i, option) in ask.options.iter().enumerate() {
        let number = (i + 1).to_string();
        question.push_str(
            &TEXT
                .option
                .replace("{n}", &number)
                .replace("{option}", option),
        );
    }
    let full = "this ask waits for a client";
    let Some((mut command, permit)) = detached(&config.ask, &question, full) else {
        return;
    };
    command.stdout(Stdio::piped());
    let (controller, (session, entry)) = (controller.clone(), ask.id.clone());
    tokio::spawn(async move {
        let _permit = permit;
        let Ok(mut child) = command.spawn() else {
            return;
        };
        let mut answer = String::new();
        if let Some(out) = child.stdout.take() {
            let _ = out.take(FILE).read_to_string(&mut answer).await;
        }
        let answered = child.wait().await.is_ok_and(|status| status.success());
        let answer = answer.trim();
        if answered && !answer.is_empty() {
            let answer = Answer::Text(answer.to_string());
            // Refused when a client answered first, which is the one way it can fail here.
            let _ = controller.answer(&session, entry, answer, None).await;
        }
    });
}

/// The configured command `argv` with `text` as its last word, silent and killed when dropped,
/// and its place in the bound of 16 asks and notifications share. None without a command, and
/// when the bound is full, which is logged saying `full`.
fn detached(
    argv: &[String],
    text: &str,
    full: &str,
) -> Option<(Command, SemaphorePermit<'static>)> {
    let (program, args) = argv.split_first()?;
    let Ok(permit) = RUNNING.try_acquire() else {
        eprintln!("razor: 16 asks or notifications are running; {full}.");
        return None;
    };
    let mut command = Command::new(program);
    command.args(args).arg(text).kill_on_drop(true);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    Some((command, permit))
}
