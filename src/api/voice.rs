//! One owner for permission, voice settings and a durable daily character count.
use super::*;
use crate::protocol::{Sound, VoiceSettings, VoiceWhen};
use tokio::fs;

#[derive(Serialize, Deserialize)]
struct Kept {
    #[serde(flatten)]
    settings: VoiceSettings,
    #[serde(default)]
    day: String,
    /// 1 once a save has written it. Before then every save wrote the old defaults of 3,000
    /// characters and $10, so each load of a file without it treats exactly those as unset (no
    /// limit); the next save writes 1, and a value saved after that stays as set.
    #[serde(default)]
    v: u8,
}
/// No spoken text crosses to the owner, only its reserved character count; or a live call's
/// `seconds`, charged at $0.05 a minute and never refused.
#[derive(Serialize, Deserialize)]
pub(super) struct Reserve {
    pub(super) characters: u32,
    pub(super) requested: bool,
    #[serde(default)]
    pub(super) seconds: f64,
}
#[derive(Serialize, Deserialize)]
pub(super) struct Granted {
    pub(super) settings: VoiceSettings,
    pub(super) sound: Sound,
    pub(super) terminal: Option<String>,
}
enum Change {
    Settings(serde_json::Map<String, Value>),
    Reserve(Reserve),
}

pub(super) fn owns(app: &App, owner: &str) -> bool {
    owner.is_empty() || owner == &*app.id
}
async fn owner(app: &App) -> Result<(), Refusal> {
    let id = app.controller.config().await.server.shared;
    if owns(app, &id) {
        return Ok(());
    }
    let found = app.peers.owner(app, &id).await;
    let name = found.map_or(id, |(owner, _)| owner.name);
    Err(bad(format!(
        "The voice settings are kept on {name}, the paired server [server] shared names; change them there."
    )))
}
/// The one read/change/write operation, serialized through rename and fsync by the owner.
async fn change(app: &App, change: Option<Change>) -> Result<Granted, Refusal> {
    owner(app).await?;
    let _lock = app.voice.lock().await;
    let path = app.home.join("voice.json");
    let bytes = match fs::read(&path).await {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => b"{}".to_vec(),
        Err(e) => return Err(other("Reading voice.json")(e).into()),
    };
    let mut kept: Kept = serde_json::from_slice(&bytes).map_err(|_| {
        Refusal(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!(
                "Fix or remove {} on the voice settings owner; it contains invalid settings.",
                path.display()
            ),
        )
    })?;
    let today = jiff::Zoned::now().date().to_string();
    if kept.day != today {
        kept.day = today;
        kept.settings.used_today = 0;
        kept.settings.call_used_today = 0.0;
    }
    if std::mem::replace(&mut kept.v, 1) == 0 {
        let s = &mut kept.settings;
        s.daily_characters = s.daily_characters.filter(|n| *n != 3000);
        s.call_daily_dollars = s.call_daily_dollars.filter(|d| *d != 10.0);
    }
    let (s, changed) = (&mut kept.settings, change.is_some());
    let (sound, terminal) = match change {
        None => (Sound::Phone, None),
        Some(Change::Settings(fields)) => {
            *s = set(s, fields)?;
            (Sound::Phone, None)
        }
        Some(Change::Reserve(r)) if r.seconds > 0.0 => {
            s.call_used_today += r.seconds / 1200.0;
            (Sound::Phone, None)
        }
        Some(Change::Reserve(r)) => voice_note(app, s, &r).await?,
    };
    if changed {
        s.gain_db = s.gain_db.clamp(-20.0, 20.0);
        s.expressiveness = s.expressiveness.clamp(0.0, 1.0);
        let bytes = serde_json::to_vec(&kept).map_err(other("Writing voice.json"))?;
        let saved = super::save(&app.home, "voice.json", &bytes).await;
        saved.map_err(other("Saving voice settings"))?;
    }
    let settings = kept.settings;
    Ok(Granted {
        settings,
        sound,
        terminal,
    })
}
/// `s` with `fields` changed, each in the form GET /v1/voice returns; the day's counts stay.
fn set(
    s: &VoiceSettings,
    fields: serde_json::Map<String, Value>,
) -> Result<VoiceSettings, Refusal> {
    let mut merged = serde_json::to_value(s).map_err(other("Reading voice settings"))?;
    fields.into_iter().for_each(|(k, v)| merged[k] = v);
    let mut new: VoiceSettings = serde_json::from_value(merged).map_err(|e| {
        bad(format!(
            "Invalid voice settings: {e}; send each field in the form GET /v1/voice returns."
        ))
    })?;
    let short = |s: &mut String, n| *s = s.chars().take(n).collect();
    short(&mut new.voice, 128);
    short(&mut new.call_voice, 128);
    new.families.retain(|family, voice| {
        short(voice, 128);
        crate::protocol::FAMILIES.contains(&family.as_str())
    });
    let time =
        |s: &str| s.len() == 5 && s.as_bytes()[2] == b':' && s.parse::<jiff::civil::Time>().is_ok();
    if (new.quiet.as_ref()).is_some_and(|q| !time(&q.from) || !time(&q.to)) {
        return Err(bad("Quiet hours must use HH:MM, from 00:00 through 23:59."));
    }
    (new.used_today, new.call_used_today) = (s.used_today, s.call_used_today);
    Ok(new)
}

/// A voice note's `r.characters`, counted toward the day once it may be made: how it sounds,
/// silent in quiet hours, else on the terminal the person used in the last minute, else on the
/// phone; and that terminal. Or why it may not be made.
async fn voice_note(
    app: &App,
    s: &mut VoiceSettings,
    r: &Reserve,
) -> Result<(Sound, Option<String>), Refusal> {
    let terminal = (app.activity.lock().await.iter())
        .filter(|(_, at)| at.elapsed() < Duration::from_secs(60))
        .max_by_key(|(_, at)| **at)
        .map(|(id, _)| id.clone());
    let left = (s.daily_characters).map_or(u32::MAX, |d| d.saturating_sub(s.used_today));
    let why = if s.when == VoiceWhen::Never {
        Some("Voice notes are off; write instead.")
    } else if s.when == VoiceWhen::Away && terminal.is_some() && !r.requested {
        Some("The person is at their computer; write instead, unless they asked for a voice note.")
    } else if r.characters == 0 {
        Some("This voice note is empty; write instead.")
    } else if r.characters > left {
        Some("The daily voice character limit is reached; write instead.")
    } else {
        None
    };
    if let Some(why) = why {
        return Err(bad(why));
    }
    let time = jiff::Zoned::now().strftime("%H:%M").to_string();
    let quiet = (s.quiet.as_ref().filter(|q| q.from != q.to)).is_some_and(|q| {
        if q.from < q.to {
            time >= q.from && time < q.to
        } else {
            time >= q.from || time < q.to
        }
    });
    s.used_today += r.characters;
    Ok(match (quiet, terminal) {
        (true, _) => (Sound::Silent, None),
        (_, Some(terminal)) => (Sound::Terminal, Some(terminal)),
        _ => (Sound::Phone, None),
    })
}

/// The settings, read through the owner on a server that names another.
pub(super) async fn get(State(app): State<App>) -> Reply<VoiceSettings> {
    Ok(Json(ask(&app, None).await?.settings))
}
pub(super) async fn put(
    State(app): State<App>,
    Json(s): Json<serde_json::Map<String, Value>>,
) -> Reply<VoiceSettings> {
    let granted = change(&app, Some(Change::Settings(s))).await?;
    Ok(Json(granted.settings))
}
pub(super) async fn reserve(
    State(app): State<App>,
    Json(r): Json<Option<Reserve>>,
) -> Reply<Granted> {
    Ok(Json(change(&app, r.map(Change::Reserve)).await?))
}
/// Settings reads and reservations make at most one request to the directly paired owner.
pub(super) async fn ask(app: &App, reserve: Option<Reserve>) -> Result<Granted, Refusal> {
    let id = app.controller.config().await.server.shared;
    if owns(app, &id) {
        return change(app, reserve.map(Change::Reserve)).await;
    }
    let body = reserve
        .map(serde_json::to_value)
        .transpose()
        .map_err(other("Asking the voice owner"))?;
    serde_json::from_value(peers::voice(app, &id, "reserve", reqwest::Method::POST, body).await?)
        .map_err(|e| other("Reading voice settings")(e).into())
}
pub(super) async fn voices(State(app): State<App>) -> Reply {
    owner(&app).await?;
    let mut command = speech::program(&app, false).await?;
    command.arg("--voices");
    let bytes = speech::run(
        command,
        std::process::Stdio::null(),
        None,
        1 << 20,
        Duration::from_secs(30),
    )
    .await
    .map_err(other("Listing voices"))?;
    Ok(Json(
        serde_json::from_slice::<Value>(&bytes).map_err(other("Reading the voice list"))?,
    ))
}
#[derive(Serialize, Deserialize)]
pub(super) struct Activity {
    terminal: String,
    active: bool,
}
/// The shared owner selects from at most 64 terminals active within a minute, on any server naming
/// it; a 65th waits for one to expire, and a close removes its token. A report crosses once.
pub(super) async fn activity(
    State(app): State<App>,
    headers: axum::http::HeaderMap,
    Json(sample): Json<Activity>,
) -> Reply {
    if sample.terminal.is_empty() || sample.terminal.len() > 128 {
        return Err(bad(
            "A terminal token must contain between 1 and 128 bytes.",
        ));
    }
    let owner = app.controller.config().await.server.shared;
    if !owns(&app, &owner) {
        if headers.contains_key(peers::HOP) {
            return Err(bad("Terminal reports forward once; correct server.shared."));
        }
        let body = serde_json::to_value(sample).map_err(other("Reporting terminal input"))?;
        return Ok(Json(
            peers::voice(&app, &owner, "activity", reqwest::Method::PUT, Some(body)).await?,
        ));
    }
    let mut active = app.activity.lock().await;
    active.retain(|_, at| at.elapsed() < Duration::from_secs(60));
    active.remove(&sample.terminal);
    if sample.active && active.len() < 64 {
        active.insert(sample.terminal, std::time::Instant::now());
    }
    Ok(Json(json!({})))
}
pub(super) fn environment(s: &VoiceSettings, family: &str) -> Vec<(&'static str, String)> {
    vec![
        (
            "RAZOR_VOICE",
            s.families.get(family).unwrap_or(&s.voice).clone(),
        ),
        ("RAZOR_GAIN_DB", s.gain_db.to_string()),
        ("RAZOR_EXPRESSIVENESS", s.expressiveness.to_string()),
    ]
}
