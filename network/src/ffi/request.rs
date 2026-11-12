//! A C client owns a current-thread runtime on one thread. Each HTTP request runs as
//! one task there that owns its stream; the C handle meets it only through bounded
//! channels, so cancelling is aborting the task: everything it owns drops, which
//! resets the stream and wakes every blocked call.
use super::{Failure, Input, RazorNetworkResponse, response_allocation};
use crate::{Client, client::Stream, lock};
use bytes::Bytes;
use http::{HeaderMap, StatusCode};
use http_body_util::BodyExt;
use hyper::body::{Body, Frame};
use iroh::{RelayUrl, SecretKey};
use std::{
    convert::Infallible,
    future::poll_fn,
    pin::Pin,
    sync::{Arc, Mutex, OnceLock, Weak},
    task::{Context, Poll},
    thread::JoinHandle,
};
use tokio::{
    runtime::Handle,
    sync::{mpsc, oneshot},
    task::AbortHandle,
};

/// Each pull queue holds one chunk and the side reading it at most one more:
/// 16 KiB of upload and 16 KiB of download per request.
const CHUNK: usize = 8192;

/// None while the request runs; then its clean end or first failure, kept from then on.
type Outcome = Mutex<Option<Result<(), Failure>>>;
type Head = (StatusCode, HeaderMap);

pub struct RazorNetworkClient {
    client: Arc<Client>,
    endpoint: iroh::Endpoint,
    runtime: Handle,
    requests: Mutex<Vec<Weak<Outcome>>>,
    stop: oneshot::Sender<()>,
    thread: JoinHandle<()>,
}

pub struct RazorNetworkRequest {
    runtime: Handle,
    task: AbortHandle,
    outcome: Arc<Outcome>,
    head: Mutex<Option<oneshot::Receiver<Head>>>,
    /// Set when the response is delivered: whether it was a 101.
    upgraded: OnceLock<bool>,
    /// The writer's queue and the HTTP body bytes still allowed; None after finish.
    upload: Mutex<Option<(mpsc::Sender<Bytes>, u64)>>,
    finished: Mutex<Option<oneshot::Receiver<Result<(), Failure>>>>,
    /// The reader's queue (None after clean EOF) and the rest of its current chunk.
    download: Mutex<(Option<mpsc::Receiver<Bytes>>, Bytes)>,
}

impl RazorNetworkClient {
    /// Binds on the runtime's own thread: binding on the caller's raised one request's
    /// peak memory by about 0.6 MiB.
    pub(super) fn open(key: SecretKey, relays: Vec<RelayUrl>) -> Result<Self, Failure> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| Failure::Other)?;
        let handle = runtime.handle().clone();
        let (stop, mut stopped) = oneshot::channel();
        let (opened, open) = std::sync::mpsc::sync_channel(1);
        let worker = move || {
            runtime.block_on(async {
                let endpoint = match crate::open(key, Some(&relays), |builder| builder).await {
                    Ok(endpoint) => endpoint,
                    Err(error) => {
                        let _ = opened.send(Err(error.into()));
                        return;
                    }
                };
                let _ = opened.send(Ok(endpoint.clone()));
                loop {
                    tokio::select! {
                        _ = &mut stopped => break,
                        // The C client serves nothing, so nothing waits in iroh's queue.
                        incoming = endpoint.accept() => match incoming {
                            Some(incoming) => incoming.refuse(),
                            None => break,
                        },
                    }
                }
                endpoint.close().await;
            })
        };
        let thread = std::thread::Builder::new().name("razor-network".into());
        let thread = thread.spawn(worker).map_err(|_| Failure::Other)?;
        match open.recv().unwrap_or(Err(Failure::Other)) {
            Ok(endpoint) => Ok(Self {
                client: Arc::new(Client::new(endpoint.clone())),
                endpoint,
                runtime: handle,
                requests: Mutex::default(),
                stop,
                thread,
            }),
            Err(failure) => {
                let _ = thread.join();
                Err(failure)
            }
        }
    }

    /// Returns at once; iroh's look for new paths runs on the client's runtime.
    pub(super) fn network_change(&self) {
        let endpoint = self.endpoint.clone();
        self.runtime
            .spawn(async move { endpoint.network_change().await });
    }

    /// Admits at most 64 live requests: a handle or its task keeps one counted.
    pub(super) fn request(&self, input: Input) -> Result<RazorNetworkRequest, Failure> {
        let mut requests = lock(&self.requests);
        requests.retain(|outcome| outcome.strong_count() > 0);
        if requests.len() == 64 {
            return Err(Failure::Other);
        }
        let outcome = Arc::new(Mutex::new(None));
        requests.push(Arc::downgrade(&outcome));
        let (upload, uploads) = mpsc::channel(1);
        let (downloads, download) = mpsc::channel(1);
        let (heads, head) = oneshot::channel();
        let (finished, finish) = oneshot::channel();
        let exchange = Exchange {
            outcome: outcome.clone(),
            upload: Arc::new(Mutex::new(uploads)),
            download: downloads,
        };
        let length = input.length;
        let task = self
            .runtime
            .spawn(exchange.run(self.client.clone(), input, heads, finished));
        Ok(RazorNetworkRequest {
            runtime: self.runtime.clone(),
            task: task.abort_handle(),
            outcome,
            head: Mutex::new(Some(head)),
            upgraded: OnceLock::new(),
            upload: Mutex::new(Some((upload, length))),
            finished: Mutex::new(Some(finish)),
            download: Mutex::new((Some(download), Bytes::new())),
        })
    }

    /// Cancels every live request, closes the endpoint and joins the runtime,
    /// whose shutdown drops every task.
    pub(super) fn close(self) {
        for outcome in lock(&self.requests).iter().filter_map(Weak::upgrade) {
            lock(&outcome).get_or_insert(Err(Failure::Cancelled));
        }
        drop(self.stop);
        let _ = self.thread.join();
    }
}

impl RazorNetworkRequest {
    fn failure(&self) -> Option<Failure> {
        lock(&self.outcome).and_then(Result::err)
    }

    fn check(&self) -> Result<(), Failure> {
        self.failure().map_or(Ok(()), Err)
    }

    pub(super) fn cancel(&self) {
        lock(&self.outcome).get_or_insert(Err(Failure::Cancelled));
        self.task.abort();
    }

    pub(super) fn response(&self) -> Result<*mut RazorNetworkResponse, Failure> {
        let head = lock(&self.head).take().ok_or(Failure::Invalid)?;
        if self.failure() == Some(Failure::Cancelled) {
            return Err(Failure::Cancelled);
        }
        let (status, headers) = self
            .runtime
            .block_on(head)
            .map_err(|_| self.failure().unwrap_or(Failure::Other))?;
        let _ = self.upgraded.set(status == StatusCode::SWITCHING_PROTOCOLS);
        Ok(response_allocation(status.as_u16(), &headers))
    }

    pub(super) fn write(&self, mut bytes: &[u8]) -> Result<(), Failure> {
        self.check()?;
        let raw = self.upgraded.get() == Some(&true);
        let mut upload = lock(&self.upload);
        let Some((queue, allowed)) = upload.as_mut() else {
            return Err(Failure::Invalid);
        };
        if !raw && bytes.len() as u64 > *allowed {
            return Err(Failure::Invalid);
        }
        while !bytes.is_empty() {
            let permit = self.runtime.block_on(queue.reserve());
            let permit = permit.map_err(|_| self.failure().unwrap_or(Failure::Uncertain))?;
            let (chunk, rest) = bytes.split_at(bytes.len().min(CHUNK));
            permit.send(Bytes::copy_from_slice(chunk));
            *allowed = allowed.saturating_sub(chunk.len() as u64);
            bytes = rest;
        }
        Ok(())
    }

    /// Half-closes after queued bytes, waiting for the peer to acknowledge the send.
    pub(super) fn finish(&self) -> Result<(), Failure> {
        self.check()?;
        if self.upgraded.get() != Some(&true) {
            return Err(Failure::Invalid);
        }
        lock(&self.upload)
            .take()
            .map(drop)
            .ok_or(Failure::Invalid)?;
        let finished = lock(&self.finished).take().ok_or(Failure::Invalid)?;
        self.runtime
            .block_on(finished)
            .unwrap_or_else(|_| Err(self.failure().unwrap_or(Failure::Uncertain)))?;
        self.check()
    }

    pub(super) fn read(&self, output: &mut [u8]) -> Result<usize, Failure> {
        let mut download = lock(&self.download);
        let (queue, rest) = &mut *download;
        let Some(reader) = queue else {
            return Ok(0);
        };
        self.check()?;
        if self.upgraded.get().is_none() {
            return Err(Failure::Invalid);
        }
        if rest.is_empty() {
            match self.runtime.block_on(reader.recv()) {
                // The empty chunk marks a clean end; later reads end at once.
                Some(chunk) if chunk.is_empty() => {
                    *queue = None;
                    return Ok(0);
                }
                Some(chunk) => *rest = chunk,
                None => {}
            }
        }
        if rest.is_empty() {
            return lock(&self.outcome)
                .unwrap_or(Err(Failure::Other))
                .map(|()| 0);
        }
        let count = output.len().min(rest.len());
        output[..count].copy_from_slice(&rest.split_to(count));
        Ok(count)
    }
}

/// The task's half of one request.
struct Exchange {
    outcome: Arc<Outcome>,
    upload: Arc<Mutex<mpsc::Receiver<Bytes>>>,
    download: mpsc::Sender<Bytes>,
}

impl Exchange {
    /// Records the outcome before the head and download queues close, so a waiting
    /// call never takes a failure for a missing response or the end of one.
    async fn run(
        self,
        client: Arc<Client>,
        input: Input,
        head: oneshot::Sender<Head>,
        finished: oneshot::Sender<Result<(), Failure>>,
    ) {
        let mut head = Some(head);
        let result = async {
            let request = input.request.map(|()| Upload(self.upload.clone()));
            let (mut response, _driver) = client
                .exchange(input.peer, input.protocol, input.connect, request)
                .await?;
            let upgraded = response.status() == StatusCode::SWITCHING_PROTOCOLS;
            if !upgraded {
                // A final response ends the upload; bytes already queued still go.
                lock(&self.upload).close();
            }
            if let Some(head) = head.take() {
                let _ = head.send((response.status(), std::mem::take(response.headers_mut())));
            }
            if !upgraded {
                let mut body = response.into_body();
                while let Some(frame) = body.frame().await {
                    if let Ok(data) = frame.map_err(|_| Failure::Uncertain)?.into_data() {
                        self.deliver(&data).await?;
                    }
                }
                return self.end().await;
            }
            let upgraded = hyper::upgrade::on(&mut response).await;
            let upgraded = upgraded
                .map_err(|_| Failure::Uncertain)?
                .downcast::<Stream>();
            let Ok(hyper::upgrade::Parts {
                mut io, read_buf, ..
            }) = upgraded
            else {
                return Err(Failure::Uncertain);
            };
            let Stream { send, recv, .. } = &mut io;
            let download = async {
                self.deliver(&read_buf).await?;
                while let Some(chunk) = recv
                    .read_chunk(CHUNK)
                    .await
                    .map_err(|_| Failure::Uncertain)?
                {
                    self.deliver(&chunk).await?;
                }
                self.end().await
            };
            let upload = async {
                let result = async {
                    while let Some(bytes) = poll_fn(|cx| lock(&self.upload).poll_recv(cx)).await {
                        send.write_all(&bytes)
                            .await
                            .map_err(|_| Failure::Uncertain)?;
                    }
                    send.finish().map_err(|_| Failure::Uncertain)?;
                    match send.stopped().await {
                        Ok(None) => Ok(()),
                        _ => Err(Failure::Uncertain),
                    }
                }
                .await;
                if result.is_err() {
                    lock(&self.outcome).get_or_insert(result);
                }
                let _ = finished.send(result);
                result
            };
            tokio::try_join!(download, upload).map(|_| ())
        };
        let result = result.await;
        lock(&self.outcome).get_or_insert(result);
    }

    /// Queues bytes for the reader as chunks of at most CHUNK, waiting for room.
    async fn deliver(&self, mut bytes: &[u8]) -> Result<(), Failure> {
        while !bytes.is_empty() {
            let permit = self
                .download
                .reserve()
                .await
                .map_err(|_| Failure::Cancelled)?;
            let (chunk, rest) = bytes.split_at(bytes.len().min(CHUNK));
            permit.send(Bytes::copy_from_slice(chunk));
            bytes = rest;
        }
        Ok(())
    }

    /// Wakes the reader with a clean end independently of the upload's outcome.
    async fn end(&self) -> Result<(), Failure> {
        let _ = self.download.send(Bytes::new()).await;
        Ok(())
    }
}

/// The HTTP request body from C. hyper stops pulling once Content-Length bytes
/// have gone; a body cut short by a final response waits instead of ending early.
struct Upload(Arc<Mutex<mpsc::Receiver<Bytes>>>);

impl Body for Upload {
    type Data = Bytes;
    type Error = Infallible;
    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, Infallible>>> {
        match lock(&self.0).poll_recv(cx) {
            Poll::Ready(Some(bytes)) => Poll::Ready(Some(Ok(Frame::data(bytes)))),
            _ => Poll::Pending,
        }
    }
}
