//! Provider sign-in streams and their transient code input; nothing enters a session record.
use super::*;
use crate::protocol::LoginProvider;

pub(super) struct Attempt {
    device: Option<String>,
    codes: mpsc::Sender<String>,
}

#[derive(Default, Deserialize)]
pub(super) struct Choice {
    #[serde(default)]
    provider: LoginProvider,
}

/// Leaving or removing the initiating device cancels the attempt and kills its CLI.
pub(super) async fn start(
    State(app): State<App>,
    Extension(gone): Extension<Gone>,
    Extension(devices::Enrolled(device)): Extension<devices::Enrolled>,
    Query(choice): Query<Choice>,
) -> Result<Response, Refusal> {
    let (show, shown) = mpsc::channel(8);
    let (codes, input) = mpsc::channel(1);
    let claude = choice.provider != LoginProvider::Chatgpt;
    if claude {
        let mut active = app.login.lock().await;
        if active.is_some() {
            return Err(Refusal(
                StatusCode::CONFLICT,
                "A Claude sign-in is already open; finish or cancel it before starting another."
                    .into(),
            ));
        }
        *active = Some(Attempt { device, codes });
    }
    let (stop, stopped) = watch::channel(false);
    let (mut end, ended) = oneshot::channel();
    tokio::spawn(async move {
        let login = async {
            match choice.provider {
                LoginProvider::Chatgpt => app
                    .responses
                    .login(show, stopped)
                    .await
                    .map_err(|e| e.to_string()),
                provider => {
                    crate::claude::login(provider == LoginProvider::Console, show, stopped, input)
                        .await
                        .map_err(|e| e.to_string())
                }
            }
        };
        tokio::pin!(login);
        let result = tokio::select! {
            result = &mut login => result,
            () = end.closed() => { let _ = stop.send(true); login.await }
        };
        if claude {
            app.login.lock().await.take();
        }
        let _ = end.send(result);
    });
    let last = ended.map(|result| match result {
        Ok(Ok(())) => Login::Done,
        Ok(Err(error)) => Login::Failed { error },
        Err(_) => Login::Failed {
            error: "The sign-in stopped before it ended.".into(),
        },
    });
    let lines = received(shown)
        .map(|text| Login::Show { text })
        .chain(stream::once(last))
        .filter_map(|line| async move { line_of(&line) });
    Ok(ndjson(lines, gone))
}

#[derive(Deserialize)]
pub(super) struct Code {
    code: String,
}

/// A code goes only to the device's running sign-in, never to a conversation.
pub(super) async fn code(
    State(app): State<App>,
    Extension(devices::Enrolled(device)): Extension<devices::Enrolled>,
    Json(code): Json<Code>,
) -> Reply {
    let refused =
        || bad("This sign-in is no longer open on this device; start /login claude again.");
    let codes = app
        .login
        .lock()
        .await
        .as_ref()
        .filter(|a| a.device == device)
        .map(|a| a.codes.clone())
        .ok_or_else(refused)?;
    let code = code.code.trim();
    if code.is_empty() || code.contains(['\r', '\n', '\0']) {
        return Err(bad("Paste the single code shown by Claude's sign-in page."));
    }
    codes.send(code.into()).await.map_err(|_| refused())?;
    Ok(Json(json!({})))
}
