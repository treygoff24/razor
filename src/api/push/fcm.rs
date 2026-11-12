//! FCM HTTP v1 request building and service-account OAuth; no response text reaches errors.
use super::{EXPIRE, Error, Message, SessionId, config, json, now};
use aws_lc_rs::{
    rand::SystemRandom,
    signature::{RSA_PKCS1_SHA256, RsaKeyPair},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use reqwest::{Client, RequestBuilder, Response, StatusCode};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::{sync::Mutex, time::Instant};

/// One credential path, expiration, project id and access token. Serialized refresh prevents
/// duplicate mints; a replaced file takes effect at the next refresh.
#[derive(Default)]
pub(super) struct Fcm(Mutex<Option<(PathBuf, Instant, String, String)>>);

#[derive(Deserialize)]
struct Account {
    project_id: String,
    client_email: String,
    private_key: String,
    private_key_id: String,
    token_uri: String,
}

#[derive(Deserialize)]
struct Grant {
    access_token: String,
    expires_in: u64,
}

fn refused() -> Error {
    Error::Refused("The FCM credentials or authorization failed; check the service-account JSON and its permissions.".into())
}

async fn account(path: &Path) -> Result<Account, Error> {
    let bytes = tokio::fs::read(path).await.map_err(|_| refused())?;
    let account: Account = serde_json::from_slice(&bytes).map_err(|_| refused())?;
    // The project becomes a URL path segment, never a second host or query.
    let project = &account.project_id;
    if project.is_empty()
        || !project
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return Err(refused());
    }
    Ok(account)
}

fn assertion(account: &Account) -> Result<String, Error> {
    let der = super::pkcs8(&account.private_key).ok_or_else(refused)?;
    let key = RsaKeyPair::from_pkcs8(&der).map_err(|_| refused())?;
    let part = |value: Value| URL_SAFE_NO_PAD.encode(value.to_string());
    let at = now() / 1000;
    let head = part(json!({"alg": "RS256", "typ": "JWT", "kid": account.private_key_id}));
    let claims = part(
        json!({"iss": account.client_email, "aud": account.token_uri,
        "scope": "https://www.googleapis.com/auth/firebase.messaging", "iat": at, "exp": at + 3600}),
    );
    let signed = format!("{head}.{claims}");
    let mut signature = vec![0; key.public_modulus_len()];
    key.sign(
        &RSA_PKCS1_SHA256,
        &SystemRandom::new(),
        signed.as_bytes(),
        &mut signature,
    )
    .map_err(|_| refused())?;
    Ok(format!("{signed}.{}", URL_SAFE_NO_PAD.encode(signature)))
}

/// The same JSON Apple receives, carried as an FCM data string; follows need their session
/// because their registration token identifies the device rather than a single activity.
pub(super) fn data(payload: &Value, session: Option<&SessionId>) -> Value {
    let mut data = json!({"payload": payload.to_string()});
    if let Some(session) = session {
        data["session"] = json!(session)
    }
    data
}

async fn body<T: DeserializeOwned>(answer: Response) -> Result<T, Error> {
    let bytes = axum::body::to_bytes(axum::body::Body::from_stream(answer.bytes_stream()), 16_384)
        .await
        .map_err(|_| refused())?;
    serde_json::from_slice(&bytes).map_err(|_| refused())
}

impl Fcm {
    pub(super) async fn request(
        &self,
        client: &Client,
        config: &config::Push,
        Message(_, push, session, priority, payload): &Message,
    ) -> Result<Option<RequestBuilder>, Error> {
        let Some(path) = &config.fcm_credentials else {
            return Ok(None);
        };
        let mut cache = self.0.lock().await;
        if cache
            .as_ref()
            .is_none_or(|(old, until, ..)| old != path || Instant::now() >= *until)
        {
            let account = account(path).await?;
            let answer = client
                .post(&account.token_uri)
                .form(&[
                    ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
                    ("assertion", &assertion(&account)?),
                ])
                .send()
                .await
                .map_err(|_| refused())?;
            if !answer.status().is_success() {
                return Err(refused());
            }
            let answer: Grant = body(answer).await?;
            if answer.access_token.is_empty() || !(61..=86_400).contains(&answer.expires_in) {
                return Err(refused());
            }
            *cache = Some((
                path.clone(),
                Instant::now() + Duration::from_secs(answer.expires_in - 60),
                account.project_id,
                answer.access_token,
            ));
        }
        // The refresh above populated the cache whenever it was empty.
        let (_, _, project, token) = cache.as_ref().unwrap();
        // HIGH for every alert that ends in a visible notification and for urgent follow updates:
        // a NORMAL data message waits while an Android phone dozes.
        let urgent = match session {
            Some(_) => *priority == 10,
            None => payload["aps"]["interruption-level"] != "passive",
        };
        let mut android = json!({"priority": if urgent { "HIGH" } else { "NORMAL" }, "ttl": format!("{EXPIRE}s")});
        // FCM keeps only a followed session's latest update while the phone is offline.
        if let Some(session) = session {
            android["collapse_key"] = json!(session);
        }
        let host = config
            .endpoint
            .as_deref()
            .unwrap_or("https://fcm.googleapis.com");
        Ok(Some(client.post(format!("{host}/v1/projects/{}/messages:send", project))
            .bearer_auth(token)
            .json(&json!({"message": {"token": push.token, "data": data(payload, session.as_ref()),
                                    "android": android}}))))
    }
}

/// Only FCM's typed token errors remove a registration; a bad payload must not remove it.
pub(super) async fn gone(status: StatusCode, answer: Response) -> bool {
    if !matches!(status, StatusCode::BAD_REQUEST | StatusCode::NOT_FOUND) {
        return false;
    }
    let Ok(answer) = body::<Value>(answer).await else {
        return false;
    };
    let Some(details) = answer["error"]["details"].as_array() else {
        return false;
    };
    details.iter().any(|d| {
        d["@type"] == "type.googleapis.com/google.firebase.fcm.v1.FcmError"
            && match status {
                StatusCode::NOT_FOUND => d["errorCode"] == "UNREGISTERED",
                StatusCode::BAD_REQUEST => d["errorCode"] == "INVALID_ARGUMENT",
                _ => false,
            }
    })
}
