//! Paired HTTP exchanges share the server endpoint; mutations always dial afresh.
use super::*;
use crate::protocol::{AgentView, ChildRequest, Live, Mail};
use axum::{
    body::{Body, Bytes},
    http::{Request, Response},
};
use futures_util::StreamExt;
use razor_network::{Connect, Protocol};
use serde_json::Value;

const READ_TIMEOUT: Duration = Duration::from_secs(2);

const CHILD_CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
const MAIL_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

impl Devices {
    /// Replaces the outgoing cache with the endpoint's lifetime, including when disabled.
    pub(crate) async fn endpoint(&self, endpoint: Option<iroh::Endpoint>) {
        *self.client.write().await = endpoint.map(|e| Arc::new(razor_network::Client::new(e)));
    }

    /// A server enrolled by code, without borrowing a stale enrollment or a phone's keys.
    pub(crate) async fn peer(&self, server: &str) -> Option<Device> {
        self.enrolled
            .read()
            .await
            .iter()
            .find(|(d, _)| d.name == server && d.is_peer())
            .filter(|_| !self.quiet.load(Relaxed))
            .map(|(d, _)| d.clone())
    }

    /// Rechecks enrollment before every exchange; revocation cancels pending requests.
    /// Cached reads share a connection; fresh mutations retain their handshake deadline.
    pub(crate) async fn request(
        &self,
        device: &Device,
        request: Request<Bytes>,
        connect: Connect,
    ) -> Result<Response<razor_network::Body>, razor_network::Error> {
        let not_sent = || razor_network::Error::NotSent {
            source: std::io::Error::other(
                "The peer is no longer enrolled or networking is disabled.",
            )
            .into(),
        };
        let peer = {
            let enrolled = self.enrolled.read().await;
            let (current, _) = enrolled
                .iter()
                .find(|(d, _)| d.name == device.name && d.keys == device.keys && d.is_peer())
                .filter(|_| !self.quiet.load(Relaxed))
                .ok_or_else(not_sent)?;
            let key = *current.keys.first().ok_or_else(not_sent)?;
            current
                .relays
                .iter()
                .cloned()
                .fold(iroh::EndpointAddr::new(key), |p, r| p.with_relay_url(r))
        };
        let client = self.client.read().await.clone().ok_or_else(not_sent)?;
        client.request(peer, Protocol::Api, connect, request).await
    }

    /// A bounded JSON read within two seconds; no recursive forwarding.
    pub(crate) async fn fetch<T: serde::de::DeserializeOwned>(
        &self,
        device: &Device,
        path: &str,
        query: &impl serde::Serialize,
        bound: usize,
    ) -> Option<T> {
        let query = serde_urlencoded::to_string(query).ok()?;
        let separator = if path.contains('?') { '&' } else { '?' };
        let path = format!("{path}{separator}{query}");
        let request = Request::get(path.as_str())
            .header(HOP, "1")
            .body(Bytes::new())
            .ok()?;
        tokio::time::timeout(READ_TIMEOUT, async {
            let response = self.request(device, request, Connect::Cached).await.ok()?;
            if !response.status().is_success() {
                return None;
            }
            let body = axum::body::to_bytes(Body::new(response.into_body()), bound)
                .await
                .ok()?;
            serde_json::from_slice(&body).ok()
        })
        .await
        .ok()?
    }

    /// One fresh status line, with both the time and the retained line bounded.
    pub(crate) async fn snapshot(&self, device: &Device) -> Option<Vec<AgentView>> {
        let request = Request::get("/v1/status?local=true")
            .body(Bytes::new())
            .ok()?;
        tokio::time::timeout(READ_TIMEOUT, async {
            let answer = self.request(device, request, Connect::Cached).await.ok()?;
            if !answer.status().is_success() {
                return None;
            }
            let mut stream = Body::new(answer.into_body()).into_data_stream();
            let mut line = Vec::new();
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.ok()?;
                let end = chunk.iter().position(|b| *b == b'\n');
                let bytes = end.map_or(chunk.len(), |n| n + 1);
                if line.len() + bytes > 8 << 20 {
                    return None;
                }
                line.extend_from_slice(&chunk[..bytes]);
                if end.is_some() {
                    return match serde_json::from_slice(&line).ok()? {
                        Live::Agents { agents, .. } => Some(agents),
                        _ => None,
                    };
                }
            }
            None
        })
        .await
        .ok()?
    }

    /// One child mutation; only a failed fresh handshake proves the operation was not sent.
    pub(crate) async fn child(
        &self,
        server: &str,
        operation: &ChildRequest,
    ) -> Result<Value, Error> {
        if self.quiet.load(Relaxed) {
            return refuse(QUIET.into());
        }
        let device = self.peer(server).await.ok_or_else(|| {
            Error::Refused(format!(
                "{server} is not paired; pair it before starting a session."
            ))
        })?;
        let uncertain = || {
            Error::Refused(format!(
                "{server} did not acknowledge the operation; it may have run. Check that server's agents list before trying again."
            ))
        };
        let (status, bytes) = match self
            .exchange(
                &device,
                "/v1/children",
                operation,
                CHILD_CONNECT_TIMEOUT,
                Duration::from_secs(30),
            )
            .await
        {
            Ok(answer) => answer,
            Err(error @ (razor_network::Error::NotSent { .. } | razor_network::Error::Refused)) => {
                return refuse(Self::refusal(server, &error).unwrap_or_else(|| {
                    format!("Could not connect to {server}; the operation was not sent.")
                }));
            }
            _ => return Err(uncertain()),
        };
        let body: Value = serde_json::from_slice(&bytes).map_err(|_| uncertain())?;
        if !status.is_success() {
            return refuse(format!(
                "{server}: {}",
                body["error"]
                    .as_str()
                    .unwrap_or("The child operation was refused.")
            ));
        }
        Ok(body)
    }

    /// One mail attempt with five seconds to connect and ten overall; never replayed.
    pub(crate) async fn mail(&self, server: &str, mail: &Mail) -> Result<String, Error> {
        if self.quiet.load(Relaxed) {
            return refuse(QUIET.into());
        }
        let device = self.peer(server).await.ok_or_else(|| {
            Error::Refused(format!(
                "No server named {server} is paired; pair it with razor devices add."
            ))
        })?;
        let (status, bytes) = match self
            .exchange(
                &device,
                "/v1/mail",
                mail,
                MAIL_CONNECT_TIMEOUT,
                Duration::from_secs(10),
            )
            .await
        {
            Ok(answer) => answer,
            Err(error @ (razor_network::Error::NotSent { .. } | razor_network::Error::Refused)) => {
                return refuse(
                    Self::refusal(server, &error)
                        .unwrap_or_else(|| format!("{server} did not answer; nothing was sent.")),
                );
            }
            _ => {
                return refuse(format!(
                    "{server} did not answer; the mail may not have arrived."
                ));
            }
        };
        let body: Value = serde_json::from_slice(&bytes).unwrap_or_default();
        match (body["session"].as_str(), body["error"].as_str()) {
            (Some(session), _) if status.is_success() => Ok(format!("{session}@{server}")),
            (_, Some(sentence)) => refuse(format!("{server}: {sentence}")),
            _ => refuse(format!(
                "{server} answered {status}; the mail may not have arrived."
            )),
        }
    }

    /// One fresh mutation with both deadlines and the shared response bound, never replayed.
    async fn exchange(
        &self,
        device: &Device,
        path: &str,
        body: &impl serde::Serialize,
        connect: Duration,
        total: Duration,
    ) -> Result<(StatusCode, Bytes), razor_network::Error> {
        let request = json_request(path, body).map_err(|source| razor_network::Error::Other {
            context: "The paired request could not be encoded.",
            source: source.into(),
        })?;
        tokio::time::timeout(total, async {
            let answer = self
                .request(device, request, Connect::Fresh { timeout: connect })
                .await?;
            let status = answer.status();
            let bytes = axum::body::to_bytes(Body::new(answer.into_body()), 512 << 10)
                .await
                .map_err(|e| razor_network::Error::Uncertain { source: e.into() })?;
            Ok((status, bytes))
        })
        .await
        .unwrap_or_else(|e| Err(razor_network::Error::Uncertain { source: e.into() }))
    }

    /// Admission refusal proves no API stream was served, even after a successful handshake.
    pub(crate) fn refusal(server: &str, error: &razor_network::Error) -> Option<String> {
        matches!(error, razor_network::Error::Refused)
            .then(|| format!("{server} refused this server's key; pair them again."))
    }
}

fn json_request(path: &str, body: &impl serde::Serialize) -> Result<Request<Bytes>, Error> {
    Request::post(path)
        .header(axum::http::header::CONTENT_TYPE, "application/json")
        .body(Bytes::from(
            serde_json::to_vec(body).map_err(other("Encoding paired request"))?,
        ))
        .map_err(other("Making paired request"))
}

impl Devices {
    /// Live path and round-trip evidence for doctor, never a claim based on stored addresses.
    pub(crate) async fn diagnostics(
        &self,
        endpoint: &iroh::Endpoint,
        relays: &[iroh::RelayUrl],
    ) -> Vec<String> {
        use iroh::Watcher;
        // A relay that admits only listed keys needs this one.
        let mut lines = vec![format!("Network key: {}.", endpoint.id())];
        for relay in endpoint.home_relay_status().get() {
            lines.push(format!(
                "Home relay: {} ({}).",
                crate::config::shown(relay.url()),
                if relay.is_connected() {
                    "connected"
                } else if relay.auth_denied_reason().is_some() {
                    "refused this server; check this server's access to it, including the password in network.relays in config.toml"
                } else {
                    "disconnected; check the relay and WebSocket access"
                }
            ));
        }
        if lines.len() == 1 {
            let relays = relays
                .iter()
                .map(crate::config::shown)
                .collect::<Vec<_>>()
                .join(", ");
            lines.push(format!(
                "Home relays: {relays} (not reached; check the relay and WebSocket access)."
            ));
        }
        for (device, _) in self.paired().await {
            let mut request = Request::new(Bytes::new());
            *request.uri_mut() = axum::http::Uri::from_static("/v1/device");
            let answer = tokio::time::timeout(
                READ_TIMEOUT,
                self.request(&device, request, Connect::Cached),
            )
            .await;
            if let Ok(Err(error)) = &answer
                && let Some(sentence) = Self::refusal(&device.name, error)
            {
                lines.push(sentence);
                continue;
            }
            let reached = matches!(answer, Ok(Ok(response)) if response.status().is_success());
            let connection = {
                let connections = self.connections.lock().unwrap_or_else(|e| e.into_inner());
                connections.iter().filter_map(|c| c.upgrade()).find(|c| {
                    c.alpn() == razor_network::API
                        && c.close_reason().is_none()
                        && device.keys.contains(&c.remote_id())
                })
            };
            let Some(connection) = connection.filter(|_| reached) else {
                lines.push(format!(
                    "Pair {}: unreachable; check that server and its relay.",
                    device.name
                ));
                continue;
            };
            let paths = connection.paths();
            if let Some(path) = paths.iter().find(|p| p.is_selected()) {
                let route = match path.remote_addr() {
                    iroh::TransportAddr::Relay(url) => {
                        format!("relayed through {}", crate::config::shown(url))
                    }
                    _ => "direct".into(),
                };
                lines.push(format!(
                    "Pair {}: {}; round trip {:.1} ms.",
                    device.name,
                    route,
                    path.rtt().as_secs_f64() * 1000.0
                ));
            } else {
                lines.push(format!(
                    "Pair {}: answering; path unavailable.",
                    device.name
                ));
            }
        }
        lines
    }
}
