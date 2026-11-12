//! One HTTP request per QUIC stream, on the controller's current endpoint.

use super::{App, Router};
use hyper::{server::conn::http1, service::service_fn};
use hyper_util::rt::{TokioIo, TokioTimer};
use iroh::{
    Endpoint,
    endpoint::{Connection, RecvStream, SendStream},
};
use std::{io, time::Duration};
use tokio::task::JoinSet;
use tower::ServiceExt;

/// 512 admitted connections share at most 512 MiB of QUIC receive payload.
const CONNECTIONS: usize = 512;
/// Pending handshakes have their own 32 slots (at most 32 MiB more receive payload).
const HANDSHAKES: usize = 32;
/// 16 KiB fits the API's small headers and caps hyper's header read buffer.
const HEADER_BYTES: usize = 16 << 10;
/// Ten seconds allows relay setup without retaining streams that never finish their headers.
const HEADER_TIMEOUT: Duration = Duration::from_secs(10);

/// Only the transport inserts this extension; HTTP headers cannot choose the calling device.
#[derive(Clone, Copy)]
pub(super) struct Device(pub(super) iroh::PublicKey);

/// Serves each endpoint the controller opens; replacing or closing one drops its connections and
/// their HTTP tasks.
pub(super) async fn serve(app: &App, router: Router) -> io::Result<()> {
    let mut endpoints = app.controller.endpoint().await;
    loop {
        let endpoint = endpoints.borrow_and_update().clone();
        let serving = async {
            if let Some(endpoint) = endpoint {
                accept(app, &router, endpoint).await;
            }
            std::future::pending::<()>().await
        };
        tokio::select! {
            () = serving => {}
            changed = endpoints.changed() => if changed.is_err() { return Ok(()) },
        }
    }
}

/// Until the endpoint closes. Pending handshakes have their own bound, below admitted
/// connections'.
async fn accept(app: &App, router: &Router, endpoint: Endpoint) {
    let (mut handshakes, mut connections) = (JoinSet::new(), JoinSet::new());
    loop {
        tokio::select! {
            incoming = endpoint.accept() => match incoming {
                None => return,
                Some(incoming) if handshakes.len() == HANDSHAKES => incoming.refuse(),
                Some(incoming) => {
                    handshakes.spawn(async move { incoming.await });
                }
            },
            result = handshakes.join_next(), if !handshakes.is_empty() => {
                let Some(Ok(Ok(connection))) = result else { continue };
                if connections.len() == CONNECTIONS {
                    connection.close(1u32.into(), b"Connection limit reached");
                    continue;
                }
                let (app, router) = (app.clone(), router.clone());
                connections.spawn(async move {
                    if connection.alpn() == razor_network::PAIRING {
                        super::pairing::serve(app, connection).await;
                    } else {
                        requests(connection, router).await;
                    }
                });
            }
            Some(_) = connections.join_next() => {}
        }
    }
}

/// Until the connection closes, which also ends its requests.
async fn requests(connection: Connection, router: Router) {
    let mut streams = JoinSet::new();
    loop {
        tokio::select! {
            stream = connection.accept_bi() => {
                let Ok((send, recv)) = stream else { return };
                streams.spawn(request(send, recv, connection.remote_id(), router.clone()));
            }
            Some(_) = streams.join_next() => {}
        }
    }
}

pub(super) async fn request(
    send: SendStream,
    recv: RecvStream,
    key: iroh::PublicKey,
    router: Router,
) {
    let stopped = send.stopped();
    let io = TokioIo::new(tokio::io::join(recv, send));
    let service = service_fn(move |mut request: hyper::Request<hyper::body::Incoming>| {
        request.extensions_mut().insert(Device(key));
        router.clone().oneshot(request.map(axum::body::Body::new))
    });
    let mut http = http1::Builder::new();
    let http = http
        .keep_alive(false)
        .half_close(true)
        .max_buf_size(HEADER_BYTES)
        .timer(TokioTimer::new())
        .header_read_timeout(HEADER_TIMEOUT)
        .serve_connection(io, service)
        .with_upgrades();
    tokio::select! {
        biased;
        _ = stopped => {}
        _ = http => {}
    }
}

/// Diagnostics are local-only: they use this server's endpoint, never a second node key.
pub(super) async fn diagnostics(
    axum::extract::State(app): axum::extract::State<App>,
    axum::Extension(caller): axum::Extension<super::devices::Enrolled>,
) -> Result<axum::Json<Vec<String>>, super::Refusal> {
    if caller.0.is_some() {
        return Err(super::bad("Run doctor from this machine's local terminal."));
    }
    let config = app.controller.config().await;
    let relays = config.network.advertised();
    let endpoint = app.controller.endpoint().await.borrow().clone();
    let lines = match endpoint {
        Some(endpoint) => app.devices.diagnostics(&endpoint, &relays).await,
        None => vec!["Network: disabled (provider_only).".into()],
    };
    Ok(axum::Json(lines))
}
