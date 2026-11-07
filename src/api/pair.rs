//! Public pairing requests and authenticated, on-demand tailnet discovery.
use super::*;
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
    if app.controller.config().await.provider_only {
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
    let server = app.controller.config().await.server;
    let port = server
        .address
        .as_deref()
        .and_then(|a| a.rsplit(':').next()?.parse::<u16>().ok());
    let mut ports = config::setup::PORTS.collect::<Vec<_>>();
    if let Some(port) = port
        && !ports.contains(&port)
    {
        ports.push(port);
    }
    let enrolled = app.devices.list().await;
    let mut targets = Vec::new();
    // 256 online peers, at most eleven ports each; no IP ranges or background polling.
    let peers = status["Peer"]
        .as_object()
        .into_iter()
        .flat_map(|p| p.values())
        .filter(|p| p["Online"] == true)
        .take(256)
        .collect::<Vec<_>>();
    for port in &ports {
        for peer in &peers {
            let ip = peer["TailscaleIPs"][0]
                .as_str()
                .and_then(|s| s.parse::<IpAddr>().ok());
            let Some(ip) = ip else { continue };
            let dns = peer["DNSName"]
                .as_str()
                .unwrap_or_default()
                .trim_end_matches('.');
            let address = SocketAddr::new(ip, *port).to_string();
            let named = format!("{dns}:{port}");
            if !enrolled.iter().any(|d| {
                d.address
                    .as_ref()
                    .is_some_and(|a| a == &address || a == &named)
            }) {
                targets.push((address, dns.to_string()));
            }
        }
    }
    let mut probes = stream::iter(targets)
        .map(|(address, dns)| {
            let client = &client;
            async move {
                let response = client
                    .get(format!("http://{address}/v1/pair"))
                    .send()
                    .await
                    .ok()?
                    .error_for_status()
                    .ok()?;
                let body = Body::from_stream(response.bytes_stream());
                let body = axum::body::to_bytes(body, 1024).await.ok()?;
                let value: Value = serde_json::from_slice(&body).ok()?;
                let name = value["name"].as_str()?;
                let suggested = dns
                    .split('.')
                    .next()
                    .filter(|n| crate::devices::valid_name(n))?;
                Some((address, (name.to_string(), suggested.to_string())))
            }
        })
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
