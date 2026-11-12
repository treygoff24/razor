//! One encrypted network client for Razor's paired servers, terminal, phone and Mac apps.
//! The endpoint is built here with Minimal, system certificate authorities and no lookup.
//! The server supplies its existing Tokio runtime and retains the endpoint for admission.
//! Each client caches connections by peer key; a dead connection is replaced at the next dial.
//! Each HTTP request uses one HTTP/1.1 QUIC stream, with keep-alive disabled.
//! An HTTP request is never replayed; a failed fresh handshake proves it was not sent.
//! Dropping an HTTP request future or HTTP response body cancels that stream alone.
//! C clients own their runtime; Rust clients use the caller's runtime.
//! C reads and writes block the caller with bounded buffers; cancellation wakes them.
//! C inputs are borrowed for the call; allocated handles and errors have explicit frees.
//!
pub use iroh::SecretKey;
use iroh::{Endpoint, RelayMode, RelayUrl, endpoint::presets};
use std::{
    error, fmt,
    sync::{Mutex, MutexGuard, PoisonError},
};

/// The protocol of Razor's API between machines.
pub const API: &[u8] = b"razor/1";
/// The protocol of pairing by code.
pub const PAIRING: &[u8] = b"razor-pair/1";

/// Admission rejected the connection before any API stream could be served.
pub const ADMISSION_REFUSED: u32 = 2;
/// Admission rejected a busy key before any API stream could be served.
pub const ADMISSION_BUSY: u32 = 3;

/// `NotSent` and `Refused` prove that the HTTP request was not sent; other failures
/// carry no retry guarantee. The underlying cause is retained without payloads.
#[derive(Debug)]
pub enum Error {
    /// This key is not enrolled on the peer; nothing was sent.
    Refused,
    /// The handshake failed or admission was busy; nothing was sent.
    NotSent {
        source: Box<dyn error::Error + Send + Sync>,
    },
    /// The peer may have received the HTTP request; it must never be replayed.
    Uncertain {
        source: Box<dyn error::Error + Send + Sync>,
    },
    /// Invalid input or another failure, with one sentence of context.
    Other {
        context: &'static str,
        source: Box<dyn error::Error + Send + Sync>,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Refused => "This key is not enrolled on that machine; pair it again.",
            Self::NotSent { .. } => "The request was not sent; try connecting again.",
            Self::Uncertain { .. } => {
                "The request may have run; check its result before sending again."
            }
            Self::Other { context, .. } => context,
        })
    }
}

impl error::Error for Error {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            Self::Refused => None,
            Self::NotSent { source } | Self::Uncertain { source } | Self::Other { source, .. } => {
                Some(source.as_ref())
            }
        }
    }
}

fn other(context: &'static str) -> Error {
    Error::Other {
        context,
        source: context.into(),
    }
}

fn uncertain(source: impl error::Error + Send + Sync + 'static) -> Error {
    Error::Uncertain {
        source: Box::new(source),
    }
}

/// A poisoned lock still holds consistent state here: every critical section is a few assignments.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Opens an endpoint on the caller's runtime, without installing an address lookup service.
/// `None` uses iroh's public relays; `Some` uses only that list (empty disables relays).
/// The key must not be used by another live endpoint. The owner calls `Endpoint::close`
/// before replacing it; enrollment, protocol admission and the key's storage are the caller's.
pub async fn open(
    key: SecretKey,
    relays: Option<&[RelayUrl]>,
    configure: impl FnOnce(iroh::endpoint::Builder) -> iroh::endpoint::Builder,
) -> Result<Endpoint, Error> {
    let relays = match relays {
        None => RelayMode::Default,
        Some(relays) => RelayMode::custom(relays.iter().cloned()),
    };
    configure(
        Endpoint::builder(presets::Minimal)
            .secret_key(key)
            .relay_mode(relays)
            // Android's own certificate checks need the app's Java set up; its apps trust no
            // user-installed authority by default, so the compiled-in roots lose nothing there.
            .ca_tls_config(if cfg!(target_os = "android") {
                iroh::tls::CaTlsConfig::embedded()
            } else {
                iroh::tls::CaTlsConfig::system()
            })
            .proxy_from_env(),
    )
    .bind()
    .await
    .map_err(|source| Error::Other {
        context: "The network endpoint could not open; check this machine's network settings.",
        source: Box::new(source),
    })
}

mod client;
pub mod ffi;
pub use client::{Body, Client, Connect, Protocol};
