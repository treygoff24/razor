//! Device authorization, enrollment routes and phone notification settings.
use super::{App, Refusal, Reply, bad};
use crate::devices::Devices;
use crate::protocol::{Device, Follow, Notifications, Push, PushService, SessionId};
use axum::{
    extract::{Extension, Json, Path, Request, State},
    http::{StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
const FOLLOWS: usize = 8;
const MUTED: usize = 256;

/// The check every request passes: over the network, the connection's enrolled key; on the
/// socket, its bearer key is the local key or an enrolled program's. The request then carries
/// that device's channel, which its streams watch.
pub(super) async fn authorize(
    State(devices): State<Arc<Devices>>,
    mut request: Request,
    next: Next,
) -> Response {
    if request.method() == axum::http::Method::GET && request.uri().path() == "/v1/mcp/callback" {
        return next.run(request).await;
    }
    let given = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or_default()
        .as_bytes();
    let enrolled = match request.extensions().get::<super::network::Device>() {
        Some(device) => devices.network(device.0).await,
        None => devices.authorize(given).await,
    };
    let Some((gone, name)) = enrolled else {
        let sentence = "This request needs a device key that works: Authorization: Bearer <key>.";
        return Refusal(StatusCode::UNAUTHORIZED, sentence.into()).into_response();
    };
    request.extensions_mut().insert(gone);
    request.extensions_mut().insert(Enrolled(name));
    next.run(request).await
}

/// The enrolled device whose key a request carried, by name; None for the local key.
#[derive(Clone)]
pub(super) struct Enrolled(pub(super) Option<String>);

/// The enrolled devices, without their keys and tokens.
pub(super) async fn list(State(app): State<App>) -> Json<Vec<Device>> {
    Json(app.devices.list().await)
}

/// Only the calling enrollment, without its key, tokens or notification settings.
pub(super) async fn identity(
    State(app): State<App>,
    Extension(device): Extension<Enrolled>,
) -> Reply {
    let name = enrolled(device)?;
    Ok(Json(json!(app.devices.identity(&name).await)))
}

/// A program to enroll, with the key it will send, or a fresh one.
#[derive(Deserialize)]
pub(super) struct Name {
    name: String,
    key: Option<String>,
}

/// Answers the enrolled program with its key.
pub(super) async fn enroll(State(app): State<App>, Json(Name { name, key }): Json<Name>) -> Reply {
    Ok(Json(json!(app.devices.enroll(name, key).await?)))
}

pub(super) async fn revoke(State(app): State<App>, Path(name): Path<String>) -> Reply {
    app.devices.revoke(&name).await?;
    Ok(Json(json!({})))
}

/// `PUT /devices/self/push`: the calling device's notification token. Answers `{}`, or
/// `{"unsent": "…"}` when this server cannot send to its environment yet; it is kept either way.
pub(super) async fn register(
    State(app): State<App>,
    Extension(device): Extension<Enrolled>,
    Json(push): Json<Push>,
) -> Reply {
    let name = phone(device, &push)?;
    let answer = super::push::unsent(&app, &push).await;
    app.devices.register(&name, push).await?;
    Ok(Json(answer))
}

pub(super) async fn unregister(
    State(app): State<App>,
    Extension(Enrolled(name)): Extension<Enrolled>,
) -> Reply {
    // The local key names no device, so it changes nothing.
    let name = name.unwrap_or_default();
    app.devices.change(&name, |d| d.push = None).await?;
    Ok(Json(json!({})))
}

/// `GET /devices/self/notifications`: what the calling device's notifications bring.
pub(super) async fn notifications(
    State(app): State<App>,
    Extension(device): Extension<Enrolled>,
) -> Reply {
    let name = enrolled(device)?;
    Ok(Json(json!(app.devices.notifications(&name).await)))
}

/// `PUT /devices/self/notifications`: replaces them, answered with what is kept.
pub(super) async fn set_notifications(
    State(app): State<App>,
    Extension(device): Extension<Enrolled>,
    Json(set): Json<Notifications>,
) -> Reply {
    let name = enrolled(device)?;
    if set.muted.len() > MUTED {
        let sentence = format!("A device mutes at most {MUTED} sessions; unmute some first.");
        return Err(bad(sentence));
    }
    let kept = json!(set);
    app.devices
        .change(&name, |d| d.notifications = Some(set))
        .await?;
    Ok(Json(kept))
}

/// `PUT /sessions/{id}/follow`: a Live Activity's token for a top-level session that is not a
/// resident, answered as `register` is.
pub(super) async fn follow(
    State(app): State<App>,
    Extension(device): Extension<Enrolled>,
    Path(session): Path<SessionId>,
    Json(push): Json<Push>,
) -> Reply {
    let name = phone(device, &push)?;
    let answer = super::push::unsent(&app, &push).await;
    let summary = app.controller.summary(&session).await?;
    if summary.child.is_some() || app.residents.timed(&summary).await {
        let sentence = "Only a top-level session that is not a resident is followed.";
        return Err(bad(sentence));
    }
    app.devices
        .change(&name, |d| {
            d.follows.retain(|f| f.session != session);
            d.follows.push(Follow { session, push });
            d.follows.drain(..d.follows.len().saturating_sub(FOLLOWS));
        })
        .await?;
    Ok(Json(answer))
}

pub(super) async fn unfollow(
    State(app): State<App>,
    Extension(Enrolled(name)): Extension<Enrolled>,
    Path(session): Path<SessionId>,
) -> Reply {
    // The local key names no device, so it changes nothing.
    let name = name.unwrap_or_default();
    app.devices
        .change(&name, |d| d.follows.retain(|f| f.session != session))
        .await?;
    Ok(Json(json!({})))
}

/// The enrolled device a registration came from, once its service and token are valid.
fn phone(device: Enrolled, push: &Push) -> Result<String, Refusal> {
    let name = enrolled(device)?;
    let valid = match (push.service, push.environment) {
        (PushService::Apns, Some(_)) => push.token.bytes().all(|b| b.is_ascii_hexdigit()),
        (PushService::Fcm, None) => push
            .token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b':')),
        _ => false,
    };
    // Apple's tokens are opaque and vary in length: a simulator's Live Activity token is 256.
    if push.token.is_empty() || push.token.len() > 1024 || !valid {
        return Err(bad(
            "Use an APNs hex token with its environment, or an FCM token without an environment; tokens have 1–1,024 characters.",
        ));
    }
    Ok(name)
}

/// The enrolled device a request came from; the local key names none.
fn enrolled(Enrolled(name): Enrolled) -> Result<String, Refusal> {
    let sentence = "This request needs an enrolled device; enroll it with razor devices add.";
    name.ok_or_else(|| bad(sentence))
}
