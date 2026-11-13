//! Acts as a phone or an unknown device for the flows. `id KEY` prints the public key of the
//! secret key in the file KEY, making the file if it is absent. `get KEY PEER RELAY PATH` sends one
//! request through the network crate and prints its status and body; on failure it prints the
//! error and its causes to standard error and exits 1. `hold KEY PEER RELAY` opens the status
//! stream, prints READY at its first line, then ENDED and how the stream ended.
//! `pair KEY PEER RELAY SECRET [SERVER_ID]` joins the code with that secret as a phone, or as a
//! server with SERVER_ID, and prints the status. `proxy KEY PEER RELAY` listens on a free local
//! port, prints it, and forwards each HTTP request through the network crate as that key, so a
//! flow talks to a server as a phone with plain HTTP; a request the network refuses is a 502, and
//! a stream the peer ends ends the same way. It stops when its standard input closes.
//! `tap KEY RELAY FIRST SECOND` stands between two servers that have each enrolled this key as
//! the other, counting what FIRST sends SECOND (see `tap` below).
//! `serve KEY RELAY` is a paired server that answers `GET /v1/status` with HTTP/1.0 and the lines
//! it has read from standard input so far, then each new line as it arrives, and anything else
//! with `[]`; it stops when standard input closes.
//! `front KEY RELAY UPSTREAM SECRET ID BEARER` stands before a plain HTTP server as a machine on
//! the network, for an app's checks whose proxy speaks plain HTTP (see `front` below).
use http_body_util::{BodyExt, Full};
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use iroh::endpoint::{RecvStream, SendStream};
use iroh::{EndpointAddr, RelayUrl, SecretKey};
use razor_network::{Client, Connect, Protocol};
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::{Arc, Mutex};
use std::{error::Error, path::Path, pin::Pin, rc::Rc, task::Poll, time::Duration};
use tokio::time::timeout;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let outcome = if matches!(args[1].as_str(), "proxy" | "serve" | "tap" | "front") {
        run(&args).await
    } else {
        timeout(Duration::from_secs(60), run(&args))
            .await
            .unwrap_or_else(|_| Err("timed out".into()))
    };
    let Err(error) = outcome else { return };
    eprintln!("{}", chain(&*error));
    std::process::exit(1);
}

/// An error and its causes, in one line.
fn chain(error: &dyn Error) -> String {
    let (mut text, mut cause) = (error.to_string(), error.source());
    while let Some(c) = cause {
        text = format!("{text}: {c}");
        cause = c.source();
    }
    text
}

async fn run(args: &[String]) -> Result<()> {
    let path = Path::new(&args[2]);
    let key: SecretKey = if path.exists() {
        std::fs::read_to_string(path)?.trim().parse()?
    } else {
        let key = SecretKey::generate();
        std::fs::write(path, key.to_bytes().map(|b| format!("{b:02x}")).concat())?;
        key
    };
    if args[1] == "id" {
        println!("{}", key.public());
        return Ok(());
    }
    if args[1] == "tap" {
        let relay = args[3].parse()?;
        return tap(key, relay, args[4].parse()?, args[5].parse()?).await;
    }
    if args[1] == "serve" {
        return serve(key, args[3].parse()?).await;
    }
    if args[1] == "front" {
        return front(key, args[3].parse()?, &args[4..]).await;
    }
    let relay: RelayUrl = args[4].parse()?;
    let endpoint = razor_network::open(key, Some(std::slice::from_ref(&relay)), |b| b).await?;
    let peer = EndpointAddr::new(args[3].parse()?).with_relay_url(relay.clone());
    if args[1] == "pair" {
        let server = args
            .get(6)
            .map_or(String::new(), |id| format!(r#""server_id":"{id}","#));
        let body = format!(
            r#"{{{server}"secret":"{}","keys":["{}"],"relays":["{relay}"]}}"#,
            args[5],
            endpoint.id()
        );
        let request = http::Request::post("/join")
            .header("content-type", "application/json")
            .body(bytes::Bytes::from(body))?;
        let connect = Connect::Fresh {
            timeout: Duration::from_secs(10),
        };
        let response = Client::new(endpoint.clone())
            .request(peer, Protocol::Pairing, connect, request)
            .await;
        let status = response?.status();
        endpoint.close().await;
        println!("{}", status.as_u16());
        return if status.is_success() {
            Ok(())
        } else {
            Err(format!("pairing answered {status}").into())
        };
    }
    if args[1] == "proxy" {
        return proxy(endpoint, peer).await;
    }
    let path = args.get(5).map_or("/v1/status", String::as_str);
    let connect = Connect::Fresh {
        timeout: Duration::from_secs(10),
    };
    let request = http::Request::get(path).body(bytes::Bytes::new())?;
    let outcome = Client::new(endpoint.clone())
        .request(peer, Protocol::Api, connect, request)
        .await;
    let result: Result<()> = async {
        let response = outcome?;
        println!("{}", response.status().as_u16());
        let mut body = response.into_body();
        while let Some(frame) = body.frame().await {
            let data = frame?.into_data().unwrap_or_default();
            print!("{}", String::from_utf8_lossy(&data));
            if args[1] == "hold" && data.contains(&b'\n') {
                println!("READY");
            }
        }
        Ok(())
    }
    .await;
    endpoint.close().await;
    if args[1] == "hold" {
        println!(
            "ENDED {}",
            result.err().map_or(String::new(), |e| chain(&*e))
        );
        return Ok(());
    }
    result
}

/// A body whose failure ends it quietly, so a stream the peer cut ends like a closed connection.
struct Ends(razor_network::Body);
impl hyper::body::Body for Ends {
    type Data = bytes::Bytes;
    type Error = std::convert::Infallible;
    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<std::result::Result<hyper::body::Frame<bytes::Bytes>, Self::Error>>> {
        Pin::new(&mut self.0)
            .poll_frame(cx)
            .map(|frame| frame.and_then(std::result::Result::ok).map(Ok))
    }
}

async fn serve(key: SecretKey, relay: RelayUrl) -> Result<()> {
    let endpoint = razor_network::open(key, Some(&[relay]), |b| {
        b.alpns(vec![razor_network::API.to_vec()])
    })
    .await?;
    let said = Arc::new(Mutex::new(Vec::<u8>::new()));
    let reading = async {
        let mut input = commands();
        while let Ok(Some(line)) = input.next_line().await {
            said.lock().unwrap().extend(format!("{line}\n").bytes());
        }
    };
    let accepting = async {
        while let Some(incoming) = endpoint.accept().await {
            let said = said.clone();
            tokio::spawn(async move {
                let Ok(connection) = incoming.await else {
                    return;
                };
                while let Ok((mut send, mut recv)) = connection.accept_bi().await {
                    let said = said.clone();
                    tokio::spawn(async move {
                        let mut head = [0; 4096];
                        let Ok(Some(n)) = recv.read(&mut head).await else {
                            return;
                        };
                        let _ = send.write_all(b"HTTP/1.0 200 OK\r\n\r\n").await;
                        if !head[..n].starts_with(b"GET /v1/status") {
                            let _ = send.write_all(b"[]").await;
                            let _ = send.finish();
                            return;
                        }
                        let mut sent = 0;
                        loop {
                            let more = said.lock().unwrap()[sent..].to_vec();
                            sent += more.len();
                            if send.write_all(&more).await.is_err() {
                                return;
                            }
                            tokio::time::sleep(Duration::from_millis(50)).await;
                        }
                    });
                }
            });
        }
    };
    tokio::select! { () = reading => Ok(()), () = accepting => Ok(()) }
}

/// Stands before the plain HTTP server at `upstream` as a machine on the network: it answers a
/// join that names `secret` with `id`, as Razor answers one, and admits the keys the join named.
/// Each API stream of theirs goes to a connection of its own to `upstream`, byte for byte but
/// for `authorization: Bearer BEARER` added after the request line, which is how the plain
/// server knows the device. The secret is taken every time, so one code pairs an app at every
/// launch. The line `refuse` on standard input forgets the keys and closes their connections, as
/// a machine does to a device it removed; the mode stops when standard input closes.
async fn front(key: SecretKey, relay: RelayUrl, args: &[String]) -> Result<()> {
    let [upstream, secret, id, bearer] = args else {
        return Err("front needs UPSTREAM SECRET ID BEARER".into());
    };
    let endpoint = razor_network::open(key, Some(&[relay]), |b| {
        b.alpns(vec![
            razor_network::API.to_vec(),
            razor_network::PAIRING.to_vec(),
        ])
    })
    .await?;
    let joined = Arc::new(Joined::default());
    let removed = Arc::new(tokio::sync::Notify::new());
    let bearer = format!("authorization: Bearer {bearer}\r\n");
    let accepting = async {
        while let Some(incoming) = endpoint.accept().await {
            let (joined, removed) = (joined.clone(), removed.clone());
            let (upstream, bearer) = (upstream.clone(), bearer.clone());
            let (secret, id) = (secret.clone(), id.clone());
            tokio::spawn(async move {
                let Ok(connection) = incoming.await else {
                    return;
                };
                if connection.alpn() == razor_network::PAIRING {
                    if let Ok((send, recv)) = connection.accept_bi().await {
                        join(send, recv, &secret, &id, &joined).await;
                        connection.closed().await;
                    }
                    return;
                }
                let refused = razor_network::ADMISSION_REFUSED.into();
                if !joined.lock().unwrap().contains(&connection.remote_id()) {
                    return connection.close(refused, b"");
                }
                let gone = removed.notified();
                tokio::pin!(gone);
                loop {
                    tokio::select! {
                        stream = connection.accept_bi() => {
                            let Ok((send, recv)) = stream else { return };
                            tokio::spawn(carry(send, recv, upstream.clone(), bearer.clone()));
                        }
                        () = &mut gone => return connection.close(refused, b""),
                    }
                }
            });
        }
    };
    let ended = async {
        let mut lines = commands();
        while let Ok(Some(line)) = lines.next_line().await {
            if line == "refuse" {
                joined.lock().unwrap().clear();
                removed.notify_waiters();
            }
        }
    };
    tokio::select! { () = accepting => Ok(()), () = ended => Ok(()) }
}

type Joined = Mutex<Vec<iroh::PublicKey>>;

/// Serves the one request of a pairing connection as Razor's `POST /join` does: to a join naming
/// the code's secret, the machine's id, with the join's keys admitted; to any other, Razor's
/// refusal. A stand-in for Razor's pairing, which enrolls nothing.
async fn join(send: SendStream, recv: RecvStream, secret: &str, id: &str, joined: &Joined) {
    let serve = service_fn(
        |request: hyper::Request<hyper::body::Incoming>| async move {
            let wanted = request.method() == http::Method::POST && request.uri().path() == "/join";
            let body = request.into_body().collect().await.map(|b| b.to_bytes());
            let offer: serde_json::Value =
                serde_json::from_slice(&body.unwrap_or_default()).unwrap_or_default();
            let keys = offer["keys"].as_array().into_iter().flatten();
            let keys = keys.filter_map(|k| k.as_str()?.parse::<iroh::PublicKey>().ok());
            let (status, answer) = if wanted && offer["secret"] == secret {
                joined.lock().unwrap().extend(keys);
                (http::StatusCode::OK, serde_json::json!({ "id": id }))
            } else {
                let refused =
                    "Pairing code refused or expired; make a fresh code on the other device.";
                (
                    http::StatusCode::INTERNAL_SERVER_ERROR,
                    serde_json::json!({ "error": refused }),
                )
            };
            let mut response =
                hyper::Response::new(Full::new(bytes::Bytes::from(answer.to_string())));
            *response.status_mut() = status;
            Ok::<_, std::convert::Infallible>(response)
        },
    );
    let io = TokioIo::new(tokio::io::join(recv, send));
    let _ = hyper::server::conn::http1::Builder::new()
        .serve_connection(io, serve)
        .await;
}

/// Carries one stream to a connection of its own to the plain server and back. Each way ends on
/// its own clean end, so an answer that ends first leaves the device sending; what the server does
/// to the connection is done to the stream, an error either way, or the device stopping the
/// answer, ends both, and nothing is answered in the server's place.
async fn carry(mut send: SendStream, mut recv: RecvStream, upstream: String, bearer: String) {
    use tokio::io::AsyncWriteExt;
    let Ok(plain) = tokio::net::TcpStream::connect(&upstream).await else {
        let _ = (send.reset(1u32.into()), recv.stop(1u32.into()));
        return;
    };
    let (mut from, mut to) = plain.into_split();
    let stopped = send.stopped();
    let up = async {
        let mut head = Vec::new();
        while !head.windows(2).any(|w| w == b"\r\n") {
            let mut buffer = [0; 4096];
            let n = recv.read(&mut buffer).await.ok().flatten().ok_or(())?;
            head.extend_from_slice(&buffer[..n]);
        }
        let line = head.windows(2).position(|w| w == b"\r\n").unwrap_or(0) + 2;
        head.splice(line..line, bearer.bytes());
        to.write_all(&head).await.map_err(drop)?;
        tokio::io::copy(&mut recv, &mut to).await.map_err(drop)?;
        to.shutdown().await.map_err(drop)
    };
    let down = async {
        tokio::io::copy(&mut from, &mut send).await.map_err(drop)?;
        send.finish().map_err(drop)
    };
    // Stopped by the device, or its connection gone; a finished answer the device has read is not.
    let cancelled = async {
        if let Ok(None) = stopped.await {
            std::future::pending::<()>().await;
        }
    };
    let ended = tokio::select! { both = async { tokio::try_join!(up, down) } => both.is_ok(), () = cancelled => false };
    if !ended {
        let _ = (send.reset(1u32.into()), recv.stop(1u32.into()));
    }
}

/// Serves each connection to `listener` by sending its requests to `peer` through `client`.
async fn forwarding(
    listener: tokio::net::TcpListener,
    client: Rc<Client>,
    peer: EndpointAddr,
) -> Result<()> {
    loop {
        let (stream, _) = listener.accept().await?;
        let (client, peer) = (client.clone(), peer.clone());
        let serve = service_fn(move |request: hyper::Request<hyper::body::Incoming>| {
            let (client, peer) = (client.clone(), peer.clone());
            async move { Ok::<_, std::convert::Infallible>(forward(&client, peer, request).await) }
        });
        tokio::task::spawn_local(async move {
            let _ = hyper::server::conn::http1::Builder::new()
                .serve_connection(TokioIo::new(stream), serve)
                .await;
        });
    }
}

/// The lines of standard input, until it closes.
fn commands() -> tokio::io::Lines<tokio::io::BufReader<tokio::io::Stdin>> {
    tokio::io::AsyncBufReadExt::lines(tokio::io::BufReader::new(tokio::io::stdin()))
}

async fn proxy(endpoint: iroh::Endpoint, peer: EndpointAddr) -> Result<()> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    println!("{}", listener.local_addr()?.port());
    let client = Rc::new(Client::new(endpoint));
    let ended = async { while let Ok(Some(_)) = commands().next_line().await {} };
    tokio::task::LocalSet::new()
        .run_until(async {
            tokio::select! { served = forwarding(listener, client, peer) => served, () = ended => Ok(()) }
        })
        .await
}

/// Stands between two servers that have each enrolled this key as the other: what it accepts from
/// one it sends on to the other. It prints the ports of two proxies that send requests as this
/// key to the first and to the second, then a line, `report` or `detail SESSION`, for each such
/// request it carries from the first to the second. The line `silent` on standard input makes it
/// refuse new connections and swallow what is sent on those open; `only PATH` makes it cut each
/// later request from the first whose path does not start with PATH.
async fn tap(
    key: SecretKey,
    relay: RelayUrl,
    first: iroh::PublicKey,
    second: iroh::PublicKey,
) -> Result<()> {
    let endpoint = razor_network::open(key, Some(std::slice::from_ref(&relay)), |b| {
        b.alpns(vec![razor_network::API.to_vec()])
    })
    .await?;
    let quiet = Arc::new(AtomicBool::new(false));
    let only = Arc::new(Mutex::new(Vec::new()));
    let address = |id| EndpointAddr::new(id).with_relay_url(relay.clone());
    let (to_first, to_second) = (address(first), address(second));
    let client = Rc::new(Client::new(endpoint.clone()));
    let (a, b) = (
        tokio::net::TcpListener::bind("127.0.0.1:0").await?,
        tokio::net::TcpListener::bind("127.0.0.1:0").await?,
    );
    println!("{} {}", a.local_addr()?.port(), b.local_addr()?.port());
    let accepting = async {
        while let Some(incoming) = endpoint.accept().await {
            if quiet.load(Relaxed) {
                incoming.refuse();
                continue;
            }
            let (endpoint, quiet, only) = (endpoint.clone(), quiet.clone(), only.clone());
            let (to_first, to_second) = (to_first.clone(), to_second.clone());
            tokio::spawn(async move {
                let Ok(connection) = incoming.await else {
                    return;
                };
                let counted = connection.remote_id() == first;
                let onward = if counted { to_second } else { to_first };
                // The tap is an already connected peer that can lose its onward connection.
                // Drop it within the paired-read budget, rather than holding the caller's mutation deadline.
                let Ok(Ok(onward)) = tokio::time::timeout(
                    Duration::from_secs(2),
                    endpoint.connect(onward, razor_network::API),
                )
                .await
                else {
                    return;
                };
                while let Ok((send, recv)) = connection.accept_bi().await {
                    let Ok((out, back)) = onward.open_bi().await else {
                        return;
                    };
                    let only = only.lock().unwrap().clone();
                    tokio::spawn(pump(recv, out, quiet.clone(), counted.then_some(only)));
                    tokio::spawn(pump(back, send, quiet.clone(), None));
                }
            });
        }
    };
    let ended = async {
        let mut lines = commands();
        while let Ok(Some(line)) = lines.next_line().await {
            if line == "silent" {
                quiet.store(true, Relaxed);
            }
            if let Some(path) = line.strip_prefix("only ") {
                *only.lock().unwrap() = format!("GET {path}").into_bytes();
            }
        }
    };
    tokio::task::LocalSet::new()
        .run_until(async {
            tokio::select! {
                served = forwarding(a, client.clone(), to_first.clone()) => served,
                served = forwarding(b, client.clone(), to_second.clone()) => served,
                () = accepting => Ok(()),
                () = ended => Ok(()),
            }
        })
        .await
}

/// Copies one direction of a stream. For the counted direction `only` is given: it prints what
/// each request it carries says, and cuts a request that does not start with `only`.
async fn pump(
    mut from: iroh::endpoint::RecvStream,
    mut to: iroh::endpoint::SendStream,
    quiet: Arc<AtomicBool>,
    only: Option<Vec<u8>>,
) {
    let (mut buffer, mut tail, mut first) = ([0; 16384], Vec::new(), true);
    while let Ok(Some(n)) = from.read(&mut buffer).await {
        let data = &buffer[..n];
        if let Some(only) = &only {
            if first && !data.starts_with(only) {
                let _ = (from.stop(1u32.into()), to.reset(1u32.into()));
                return;
            }
            let joined = [tail.as_slice(), data].concat();
            for _ in joined.windows(17).filter(|w| *w == br#""action":"report""#) {
                println!("report");
            }
            tail = joined[joined.len().saturating_sub(16)..].to_vec();
            if let Some(rest) = data.strip_prefix(b"GET /v1/sessions/").filter(|_| first)
                && let Some(end) = rest.iter().position(|b| *b == b'/')
                && rest[end..].starts_with(b"/agent")
                && matches!(rest.get(end + 6), Some(b' ' | b'?'))
            {
                println!("detail {}", String::from_utf8_lossy(&rest[..end]));
            }
            first = false;
        }
        if !quiet.load(Relaxed) && to.write_all(data).await.is_err() {
            break;
        }
    }
    let _ = to.finish();
}

async fn forward(
    client: &Client,
    peer: EndpointAddr,
    request: hyper::Request<hyper::body::Incoming>,
) -> hyper::Response<http_body_util::Either<Full<bytes::Bytes>, Ends>> {
    use http::header::{CONNECTION, CONTENT_LENGTH, HOST, TRANSFER_ENCODING};
    let (mut parts, body) = request.into_parts();
    for name in [CONNECTION, CONTENT_LENGTH, HOST, TRANSFER_ENCODING] {
        parts.headers.remove(name);
    }
    let sent = async {
        let body = body.collect().await?.to_bytes();
        let connect = Connect::Cached;
        Ok::<_, Box<dyn Error>>(
            client
                .request(
                    peer,
                    Protocol::Api,
                    connect,
                    http::Request::from_parts(parts, body),
                )
                .await?,
        )
    };
    match sent.await {
        Ok(response) => {
            let (mut parts, body) = response.into_parts();
            for name in [CONNECTION, CONTENT_LENGTH, TRANSFER_ENCODING] {
                parts.headers.remove(name);
            }
            hyper::Response::from_parts(parts, http_body_util::Either::Right(Ends(body)))
        }
        Err(error) => {
            let text = chain(&*error);
            let mut response =
                hyper::Response::new(http_body_util::Either::Left(Full::new(text.into())));
            *response.status_mut() = http::StatusCode::BAD_GATEWAY;
            response
        }
    }
}
