//! Local code commands and the separate, bounded pairing protocol.
use super::*;
use crate::devices::pairing::{self, BYTES, Code, DEADLINE, Exchange};
use devices::Enrolled;
use iroh::{Endpoint, EndpointAddr, TransportAddr, Watcher};
use razor_network::{Client, Connect, Protocol};

#[derive(Deserialize)]
pub(super) struct Command {
    name: String,
    code: Option<String>,
}

fn local(Enrolled(name): Enrolled) -> Result<(), Refusal> {
    if name.is_some() {
        return Err(Refusal(
            StatusCode::FORBIDDEN,
            "Pair devices from this machine's local terminal.".into(),
        ));
    }
    Ok(())
}
async fn endpoint(app: &App) -> Result<Endpoint, Refusal> {
    app.controller
        .endpoint()
        .await
        .borrow()
        .clone()
        .ok_or_else(|| Refusal(StatusCode::FORBIDDEN, crate::devices::QUIET.into()))
}
/// The first line carries the code; the second is its one final result. Disconnecting withdraws
/// the code.
pub(super) async fn add(
    State(app): State<App>,
    Extension(device): Extension<Enrolled>,
    Extension(gone): Extension<Gone>,
    Json(command): Json<Command>,
) -> Result<Response, Refusal> {
    local(device)?;
    let relays = app.controller.config().await.network.advertised();
    let endpoint = endpoint(&app).await?;
    let mut status = endpoint.home_relay_status();
    let answered = async {
        while !status
            .get()
            .iter()
            .any(|s| s.is_connected() || s.last_error().is_some())
        {
            if status.updated().await.is_err() {
                break;
            }
        }
    };
    let _ = tokio::time::timeout(Duration::from_secs(5), answered).await;
    let (connected, lines) = Devices::relay_status(&endpoint, &relays);
    let warning = (!connected).then(|| {
        format!(
            "{} A device elsewhere may not reach this server until that is fixed.",
            lines.join(" ")
        )
    });
    let (code, done) = app
        .devices
        .offer(command.name, endpoint.id(), relays)
        .await?;
    let code = json!({"code": code, "warning": warning}).to_string().into();
    let result = async move {
        json!({"result": app.devices.outcome(done).await})
            .to_string()
            .into()
    };
    Ok(ndjson(
        stream::iter([code]).chain(stream::once(result)),
        gone,
    ))
}

/// B saves A only after the authenticated exchange returns A's server id.
pub(super) async fn join(
    State(app): State<App>,
    Extension(device): Extension<Enrolled>,
    Json(command): Json<Command>,
) -> Reply {
    local(device)?;
    if !crate::devices::valid_name(&command.name) {
        return Err(bad(
            "A device name is 1 to 32 lowercase letters, digits and -.",
        ));
    }
    let code = Code::parse(command.code.as_deref().unwrap_or_default())?;
    app.devices
        .check_network_name(&command.name, code.key)
        .await?;
    let config = app.controller.config().await;
    pairing::relays(&code.relays, config.network.relays().as_deref())?;
    let endpoint = endpoint(&app).await?;
    let relays = config.network.advertised();
    pairing::relays(&relays, None)?;
    let exchange = code.exchange(endpoint.id(), relays, app.id.to_string());
    let body = serde_json::to_vec(&exchange).map_err(other("Making the pairing request"))?;
    let request = axum::http::Request::post("/join")
        .header(header::CONTENT_TYPE, "application/json")
        .body(body.into())
        .map_err(other("Making the pairing request"))?;
    let unpaired = |what| {
        bad(format!(
            "Pairing {what}; this device is not paired, so make a fresh code and try again."
        ))
    };
    let peer = EndpointAddr::from_parts(
        code.key,
        code.relays.iter().cloned().map(TransportAddr::Relay),
    );
    let fresh = Connect::Fresh { timeout: DEADLINE };
    let client = Client::new(endpoint);
    let exchange = async {
        let response = (client.request(peer, Protocol::Pairing, fresh, request))
            .await
            .map_err(|_| unpaired("was not acknowledged"))?;
        let status = response.status();
        let bytes = axum::body::to_bytes(Body::new(response.into_body()), BYTES)
            .await
            .map_err(|_| unpaired("reply was lost"))?;
        let reply: Value =
            serde_json::from_slice(&bytes).map_err(|_| unpaired("reply was invalid"))?;
        if !status.is_success() {
            return Err(bad(reply["error"].as_str().unwrap_or(
                "Pairing was refused; make a fresh code and try again.",
            )));
        }
        let id = (reply["id"].as_str())
            .filter(|id| pairing::server_id(id))
            .ok_or_else(|| unpaired("reply had no server id"))?;
        let (devices, id) = (app.devices.clone(), Some(id.to_owned()));
        let (name, keys, relays) = (command.name, vec![code.key], code.relays);
        let enrolling = async move { devices.enroll_network(name, keys, relays, id).await };
        tokio::spawn(enrolling)
            .await
            .map_err(other("Finishing pairing"))??;
        Ok(Json(json!({})))
    };
    tokio::time::timeout(DEADLINE, exchange)
        .await
        .map_err(|_| unpaired("timed out"))?
}

/// Unknown keys reach only this router, with one request, one deadline and a small body.
pub(super) async fn serve(app: App, connection: iroh::endpoint::Connection) {
    let router = Router::new()
        .route("/join", post(accept))
        .layer(DefaultBodyLimit::max(BYTES))
        .with_state(app);
    let exchange = async {
        if let Ok((send, recv)) = connection.accept_bi().await {
            super::network::request(send, recv, connection.remote_id(), router).await;
            connection.closed().await;
        }
    };
    let _ = tokio::time::timeout(DEADLINE, exchange).await;
}
async fn accept(
    State(app): State<App>,
    Extension(key): Extension<super::network::Device>,
    Json(exchange): Json<Exchange>,
) -> Reply {
    let allowed = app.controller.config().await.network.relays();
    app.devices.accept_pairing(key.0, exchange, allowed).await?;
    Ok(Json(json!({"id": app.id})))
}

/// Prints the QR and its exact copyable text, then waits for the final status or Ctrl-C.
pub async fn add_command(client: &reqwest::Client, key: &str, name: &str) -> Result<String, Error> {
    let request = client
        .post(format!("{LOCAL}/v1/pairing"))
        .bearer_auth(key)
        .json(&json!({"name": name}));
    let mut response = super::client::send(request).await?;
    let mut buffered = Vec::new();
    loop {
        let chunk = tokio::select! {
            chunk = response.chunk() => chunk.map_err(other("Waiting for pairing"))?,
            _ = tokio::signal::ctrl_c() => return Ok("Pairing code withdrawn.".into()),
        };
        let Some(chunk) = chunk else {
            return Err(Error::Refused(
                "Pairing ended without an answer; make a fresh code and try again.".into(),
            ));
        };
        buffered.extend_from_slice(&chunk);
        if buffered.len() > BYTES {
            return Err(Error::Refused(
                "Pairing reply was too large; restart pairing.".into(),
            ));
        }
        while let Some(n) = buffered.iter().position(|b| *b == b'\n') {
            let line: Value = serde_json::from_slice(&buffered[..n]).map_err(|_| {
                Error::Refused("Pairing reply was invalid; restart pairing.".into())
            })?;
            buffered.drain(..=n);
            if let Some(warning) = line["warning"].as_str() {
                eprintln!("{warning}");
            }
            if let Some(code) = line["code"].as_str() {
                let qr = qrcode::QrCode::with_error_correction_level(code, qrcode::EcLevel::L)
                    .map_err(other("Drawing the pairing code"))?;
                println!(
                    "\x1b[30;47m{}\x1b[0m\n{code}",
                    qr.render::<qrcode::render::unicode::Dense1x2>().build()
                );
                use std::io::Write;
                std::io::stdout()
                    .flush()
                    .map_err(other("Showing the pairing code"))?;
            }
            if let Some(result) = line["result"].as_str() {
                return Ok(result.into());
            }
        }
    }
}
/// Reads a bounded code from stdin; it never enters argv or a URL.
pub async fn join_command(
    client: &reqwest::Client,
    key: &str,
    name: &str,
) -> Result<String, Error> {
    use tokio::io::AsyncBufReadExt;
    eprintln!("Paste the pairing code:");
    let mut code = String::new();
    let mut input =
        tokio::io::BufReader::new(tokio::io::stdin().take((pairing::CODE_BYTES + 2) as u64));
    input
        .read_line(&mut code)
        .await
        .map_err(other("Reading the pairing code"))?;
    let request = client
        .post(format!("{LOCAL}/v1/pairing/join"))
        .bearer_auth(key)
        .json(&json!({"name": name, "code": code.trim()}));
    let _: Value = super::client::json(request).await?;
    Ok(format!("{name} joined"))
}
