use crate::{API, Error, PAIRING, lock, other, uncertain};
use bytes::Bytes;
use http::{HeaderValue, Request, Response, Version, header};
use http_body_util::{BodyExt, Full, combinators::UnsyncBoxBody};
use hyper::body::{Body as HttpBody, Frame, Incoming, SizeHint};
use iroh::{
    Endpoint, EndpointAddr, EndpointId,
    endpoint::{
        ConnectOptions, Connection, QuicTransportConfig, RecvStream, SendStream, WriteError,
    },
};
use std::{
    collections::VecDeque,
    error, io,
    pin::Pin,
    sync::{Arc, Mutex, OnceLock, Weak},
    task::{Context, Poll, ready},
    time::Duration,
};
use tokio::{sync::watch, task::JoinSet};

/// The API and pairing have separate authenticated protocols.
#[derive(Clone, Copy)]
pub enum Protocol {
    Api,
    Pairing,
}
/// Cached shares an API dial; Fresh never enters the cache and bounds its handshake.
/// Pairing requires Fresh. Its timeout must be positive and wraps iroh's timeout.
#[derive(Clone, Copy)]
pub enum Connect {
    Cached,
    Fresh { timeout: Duration },
}
/// A streamed response; dropping it explicitly cancels its QUIC stream.
pub type Body = UnsyncBoxBody<Bytes, Error>;

/// Cached callers cannot set a deadline for the handshake alone, so an unreachable
/// peer would otherwise hold them for iroh's own timeout; this one ends it NotSent.
const CACHED_HANDSHAKE: Duration = Duration::from_secs(5);
type Dial = Result<Connection, Arc<Error>>;
/// The cached dial runs to its own bound even when every current waiter leaves.
type Slot = OnceLock<watch::Receiver<Option<Dial>>>;

/// Shares its caller's endpoint and runtime, retaining at most 16 cached and
/// 16 active uncached/evicted connections, including in-progress dials.
/// Cached connections are indexed by authenticated key; concurrent misses share
/// a handshake. A dead connection is replaced at the next dial. A full cache
/// evicts its oldest reference while active streams keep that connection alive.
/// Exhaustion returns Other before dialing. Dropping it releases the cache;
/// the endpoint owner closes the endpoint.
pub struct Client {
    endpoint: Endpoint,
    cache: Mutex<Cache>,
}

#[derive(Default)]
struct Cache {
    cached: VecDeque<(EndpointId, Arc<Slot>)>,
    active: Vec<Weak<Slot>>,
}

impl Cache {
    /// The slot to dial or share, or None when it would need a 17th active connection.
    fn reserve(&mut self, key: EndpointId, connect: Connect) -> Option<Arc<Slot>> {
        self.active.retain(|slot| slot.strong_count() > 0);
        if let Connect::Fresh { .. } = connect {
            if self.active.len() == 16 {
                return None;
            }
            let slot = Arc::<Slot>::default();
            self.active.push(Arc::downgrade(&slot));
            return Some(slot);
        }
        if let Some(i) = self.cached.iter().position(|(k, _)| *k == key) {
            let (_, found) = &self.cached[i];
            let live =
                |r: &Result<Connection, _>| r.as_ref().is_ok_and(|c| c.close_reason().is_none());
            if found
                .get()
                .is_none_or(|result| result.borrow().as_ref().is_none_or(live))
            {
                return Some(found.clone());
            }
            // A failed or closed connection gives up its place at once.
            self.cached.remove(i);
        }
        if self.cached.len() == 16 {
            let busy = Arc::strong_count(&self.cached[0].1) > 1;
            if busy && self.active.len() == 16 {
                return None;
            }
            if let Some((_, oldest)) = self.cached.pop_front()
                && busy
            {
                self.active.push(Arc::downgrade(&oldest));
            }
        }
        let slot = Arc::<Slot>::default();
        self.cached.push_back((key, slot.clone()));
        Some(slot)
    }
}

impl Client {
    /// Shares the endpoint without starting a runtime or thread.
    pub fn new(endpoint: Endpoint) -> Self {
        Self {
            endpoint,
            cache: Mutex::default(),
        }
    }

    /// Opens one HTTP/1.1 stream with Host/framing supplied and keep-alive off.
    /// Accepts only origin-form URIs and refuses conflicting headers before dialing.
    /// Bytes are not copied. Dial failures and admission refusal prove NotSent; other
    /// exchange/body failures are Uncertain. No request is ever replayed.
    /// Dropping the future or body cancels only its stream; callers own deadlines
    /// and total response limits.
    pub async fn request(
        &self,
        peer: EndpointAddr,
        protocol: Protocol,
        connect: Connect,
        request: Request<Bytes>,
    ) -> Result<Response<Body>, Error> {
        let length = request.body().len() as u64;
        let mut request = request.map(Full::new);
        prepare(&mut request, length)?;
        let (response, driver) = self.exchange(peer, protocol, connect, request).await?;
        Ok(response.map(|body| {
            Guarded {
                body,
                _driver: driver,
            }
            .boxed_unsync()
        }))
    }

    /// Sends a prepared request on a new stream. The returned set drives the stream
    /// and any upgrade; dropping it, or the stream after an upgrade, cancels the stream.
    pub(crate) async fn exchange<B>(
        &self,
        peer: EndpointAddr,
        protocol: Protocol,
        connect: Connect,
        request: Request<B>,
    ) -> Result<(Response<Incoming>, JoinSet<()>), Error>
    where
        B: HttpBody<Data = Bytes> + Send + 'static,
        B::Error: Into<Box<dyn error::Error + Send + Sync>>,
    {
        let (timeout, alpn) = match (connect, protocol) {
            (Connect::Fresh { timeout }, Protocol::Api) if !timeout.is_zero() => (timeout, API),
            (Connect::Fresh { timeout }, Protocol::Pairing) if !timeout.is_zero() => {
                (timeout, PAIRING)
            }
            (Connect::Cached, Protocol::Api) => (CACHED_HANDSHAKE, API),
            _ => {
                return Err(other(
                    "Choose a positive fresh connection deadline for pairing.",
                ));
            }
        };
        let slot = lock(&self.cache).reserve(peer.id, connect).ok_or_else(|| {
            other("Too many active network connections; close a request and try again.")
        })?;
        let endpoint = self.endpoint.clone();
        let dial = async move {
            // No incoming streams, 1 MiB received per connection and 256 KiB per stream,
            // and the full handshake before any request byte (no 0-RTT).
            let transport = QuicTransportConfig::builder()
                .max_concurrent_bidi_streams(0u32.into())
                .max_concurrent_uni_streams(0u32.into())
                .receive_window(1_048_576u32.into())
                .stream_receive_window(262_144u32.into())
                .build();
            let options = ConnectOptions::new().with_transport_config(transport);
            let handshake = async {
                let connecting = endpoint
                    .connect_with_opts(peer, alpn, options)
                    .await
                    .map_err(|source| Error::NotSent {
                        source: source.into(),
                    })?;
                connecting.await.map_err(|source| match source {
                    iroh::endpoint::ConnectingError::ConnectionError {
                        source: iroh::endpoint::ConnectionError::ApplicationClosed(close),
                        ..
                    } if close.error_code == crate::ADMISSION_REFUSED.into() => Error::Refused,
                    source => Error::NotSent {
                        source: source.into(),
                    },
                })
            };
            tokio::time::timeout(timeout, handshake)
                .await
                .unwrap_or_else(|source| {
                    Err(Error::NotSent {
                        source: source.into(),
                    })
                })
                .map_err(Arc::new)
        };
        let connection = match connect {
            Connect::Fresh { .. } => dial.await,
            Connect::Cached => {
                let held = slot.clone();
                let mut result = slot
                    .get_or_init(|| {
                        let (sender, receiver) = watch::channel(None);
                        tokio::spawn(async move {
                            sender.send_replace(Some(dial.await));
                            drop(held);
                        });
                        receiver
                    })
                    .clone();
                loop {
                    if let Some(answer) = result.borrow_and_update().clone() {
                        break answer;
                    }
                    if let Err(error) = result.changed().await {
                        break Err(Arc::new(Error::NotSent {
                            source: error.into(),
                        }));
                    }
                }
            }
        };
        let connection = connection.map_err(|source| match source.as_ref() {
            Error::Refused => Error::Refused,
            _ => Error::NotSent {
                source: Box::new(source),
            },
        })?;
        let failed = |source: Box<dyn error::Error + Send + Sync>| match connection.close_reason() {
            Some(iroh::endpoint::ConnectionError::ApplicationClosed(close))
                if close.error_code == crate::ADMISSION_REFUSED.into() =>
            {
                Error::Refused
            }
            Some(iroh::endpoint::ConnectionError::ApplicationClosed(close))
                if close.error_code == crate::ADMISSION_BUSY.into() =>
            {
                Error::NotSent { source }
            }
            _ => Error::Uncertain { source },
        };
        let (send, recv) = connection.open_bi().await.map_err(|e| failed(e.into()))?;
        let stream = Stream {
            send,
            recv,
            _slot: slot,
        };
        let (mut sender, http_connection) = hyper::client::conn::http1::Builder::new()
            .max_headers(1024)
            .max_buf_size(65_536)
            .handshake(stream)
            .await
            .map_err(|e| failed(e.into()))?;
        let mut driver = JoinSet::new();
        driver.spawn(async move {
            let _ = http_connection.with_upgrades().await;
        });
        let response = sender
            .send_request(request)
            .await
            .map_err(|e| failed(e.into()))?;
        Ok((response, driver))
    }
}

/// Refuses a remote URL or caller-supplied framing, then supplies Host, Connection and length.
pub(crate) fn prepare<B>(request: &mut Request<B>, length: u64) -> Result<(), Error> {
    let path = request.uri().path();
    if request.uri().scheme().is_some()
        || request.uri().authority().is_some()
        || !path.starts_with('/')
        || path.starts_with("//")
    {
        return Err(other("Use a path and query without a remote URL."));
    }
    let headers = request.headers_mut();
    let framing = [
        header::HOST,
        header::CONNECTION,
        header::CONTENT_LENGTH,
        header::TRANSFER_ENCODING,
    ];
    if framing.iter().any(|name| headers.contains_key(name)) {
        return Err(other(
            "Let the network client supply Host, Connection and body framing.",
        ));
    }
    let connection = match headers.contains_key(header::UPGRADE) {
        true => "upgrade",
        false => "close",
    };
    headers.insert(header::HOST, HeaderValue::from_static("razor"));
    headers.insert(header::CONNECTION, HeaderValue::from_static(connection));
    headers.insert(header::CONTENT_LENGTH, length.into());
    *request.version_mut() = Version::HTTP_11;
    Ok(())
}

/// One HTTP exchange's QUIC stream, holding its connection's slot. Dropping it resets
/// the send side and stops the receive side: a dropped send stream would otherwise
/// finish cleanly, and the peer would see a cancelled request as complete.
pub(crate) struct Stream {
    pub(crate) send: SendStream,
    pub(crate) recv: RecvStream,
    _slot: Arc<Slot>,
}

impl Drop for Stream {
    fn drop(&mut self) {
        let _ = self.send.reset(1u32.into());
        let _ = self.recv.stop(1u32.into());
    }
}

impl hyper::rt::Read for Stream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        mut buf: hyper::rt::ReadBufCursor<'_>,
    ) -> Poll<io::Result<()>> {
        let mut bytes = [0; 8192];
        let capacity = buf.remaining().min(bytes.len());
        let read = ready!(self.recv.poll_read(cx, &mut bytes[..capacity]))?;
        buf.put_slice(&bytes[..read]);
        Poll::Ready(Ok(()))
    }
}

impl hyper::rt::Write for Stream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        match ready!(Pin::new(&mut self.send).poll_write(cx, buf)) {
            // A peer that stops reading the body may already have sent its response;
            // the rest of the body is discarded so hyper still reads that response.
            Err(WriteError::Stopped(_)) => Poll::Ready(Ok(buf.len())),
            written => Poll::Ready(written.map_err(Into::into)),
        }
    }
    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(self.send.finish().map_err(Into::into))
    }
}

/// A Rust response body, holding the task that drives its stream.
struct Guarded {
    body: Incoming,
    _driver: JoinSet<()>,
}

impl HttpBody for Guarded {
    type Data = Bytes;
    type Error = Error;
    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, Error>>> {
        Pin::new(&mut self.body).poll_frame(cx).map_err(uncertain)
    }
    fn is_end_stream(&self) -> bool {
        self.body.is_end_stream()
    }
    fn size_hint(&self) -> SizeHint {
        self.body.size_hint()
    }
}
