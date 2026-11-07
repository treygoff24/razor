//! Public pairing requests and authenticated, on-demand tailnet discovery.
use super::*;
use crate::protocol::Device;
use std::net::{IpAddr, SocketAddr};

/// This server's name only; no enrollment, session or credential data.
pub(super) async fn identity(State(app): State<App>) -> Json<Value> {
    Json(json!({"name": app.name}))
}

#[derive(Deserialize)]
pub(super) struct Request {
    address: SocketAddr,
    key: String,
}

/// At most 1 KiB enters the bounded, expiring hold; this never enrolls a device.
pub(super) async fn hold(
    State(app): State<App>,
    peer: Option<Extension<axum::extract::ConnectInfo<SocketAddr>>>,
    Json(request): Json<Request>,
) -> Reply {
    if !crate::devices::valid_key(&request.key) {
        return Err(bad(
            "A pairing key is the 64 hex digits the other server printed.",
        ));
    }
    if !peer.is_some_and(|Extension(peer)| peer.0.ip() == request.address.ip()) {
        return Err(bad(
            "The pairing address must belong to the requesting machine.",
        ));
    }
    app.devices.hold(request.address, request.key);
    Ok(Json(json!({})))
}

/// Online tailnet Razor servers, bounded in peers, concurrency, reply bytes and total time.
pub(super) async fn discover(State(app): State<App>) -> Reply {
    let config = app.controller.config().await;
    if config.provider_only {
        return Err(Refusal(StatusCode::FORBIDDEN, crate::devices::QUIET.into()));
    }
    let Some(status) = config::setup::tailnet().await else {
        return Ok(Json(
            json!({"servers": [], "notice": "Tailscale is not running here; set server.address on both machines and pair with razor devices add <name> --address <host:port>."}),
        ));
    };
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_millis(1500))
        .build()
        .map_err(other("Starting tailnet discovery"))?;
    let targets = targets(&status, &config.server, &app.devices.list().await);
    let mut probes = stream::iter(targets)
        .map(|(address, dns)| probe(&client, address, dns))
        .buffer_unordered(64);
    let mut found = BTreeMap::new();
    // Keep completed replies if a large tailnet exceeds the five-second scan budget.
    let complete = tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(result) = probes.next().await {
            if let Some((address, name)) = result {
                found.insert(address, name);
            }
        }
    })
    .await
    .is_ok();
    let servers = found
        .into_iter()
        .map(|(address, (name, suggested))| json!({"address": address, "name": name, "suggested": suggested}))
        .collect::<Vec<_>>();
    Ok(Json(
        json!({"servers": servers, "notice": if complete { "" } else { "Discovery timed out; run razor devices find again to look again." }}),
    ))
}

/// Each online peer's address and name at each port a server may listen on, setup's and this
/// server's own, unless a device is enrolled at it.
fn targets(status: &Value, server: &config::Server, enrolled: &[Device]) -> Vec<(String, String)> {
    let own = (server.address.as_deref()).and_then(|a| a.rsplit(':').next()?.parse::<u16>().ok());
    let mut ports = config::setup::PORTS.collect::<Vec<_>>();
    ports.extend(own.filter(|port| !ports.contains(port)));
    // 256 online peers, at most eleven ports each; no IP ranges or background polling.
    let peers: Vec<_> = (status["Peer"].as_object().into_iter())
        .flat_map(|p| p.values())
        .filter(|p| p["Online"] == true)
        .take(256)
        .filter_map(|p| {
            let ip = p["TailscaleIPs"][0].as_str()?.parse::<IpAddr>().ok()?;
            let dns = p["DNSName"].as_str().unwrap_or_default();
            Some((ip, dns.trim_end_matches('.')))
        })
        .collect();
    let mut targets = Vec::new();
    for port in &ports {
        for (ip, dns) in &peers {
            let address = SocketAddr::new(*ip, *port).to_string();
            let named = format!("{dns}:{port}");
            let paired =
                |d: &Device| (d.address.as_ref()).is_some_and(|a| a == &address || a == &named);
            if !enrolled.iter().any(paired) {
                targets.push((address, dns.to_string()));
            }
        }
    }
    targets
}

/// The name a Razor server at `address` answers with, in at most 1 KiB, and the device name its
/// tailnet name suggests; None for anything else.
async fn probe(
    client: &reqwest::Client,
    address: String,
    dns: String,
) -> Option<(String, (String, String))> {
    let response = client
        .get(format!("http://{address}/v1/pair"))
        .send()
        .await
        .ok()?;
    let body = Body::from_stream(response.error_for_status().ok()?.bytes_stream());
    let value: Value =
        serde_json::from_slice(&axum::body::to_bytes(body, 1024).await.ok()?).ok()?;
    let name = value["name"].as_str()?.to_string();
    let suggested = dns
        .split('.')
        .next()
        .filter(|n| crate::devices::valid_name(n))?;
    Some((address, (name, suggested.to_string())))
}
