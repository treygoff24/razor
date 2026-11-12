//! One endpoint for this server, rebuilt under the controller's config lock.

use super::{Error, other};
use crate::config::Config;
use iroh::{Endpoint, RelayUrl, SecretKey};
use std::{os::unix::fs::PermissionsExt, path::Path, sync::Arc};
use tokio::sync::watch;

/// Shared by remote terminals, each retaining status, session and file streams.
const STREAMS: u32 = 256;
/// QUIC's ceiling; iroh's path idle timeout releases dead peers first (30–40 seconds measured).
const IDLE_MS: u32 = 120_000;

#[derive(Default)]
pub(super) struct Network {
    pub(super) endpoint: watch::Sender<Option<Endpoint>>,
    closed: bool,
    relays: Option<Vec<RelayUrl>>,
}

impl Network {
    /// Closes the endpoint permanently for this controller, including during config reloads.
    /// Only a `connected` endpoint waits for its peers to hear; otherwise the drain is iroh's own
    /// address-discovery connections to the relays, a second the process exit does not need.
    pub(super) async fn close(&mut self, connected: bool) {
        self.closed = true;
        if let Some(endpoint) = self.endpoint.send_replace(None).filter(|_| connected) {
            endpoint.close().await;
        }
    }

    /// Called on every config read, so the first one after start opens the endpoint. Closes
    /// before disabling or replacing; a failure leaves no endpoint, reaches the status notice and
    /// is retried at the next read. No address lookup service is installed.
    pub(super) async fn configure(
        &mut self,
        home: &Path,
        config: &Config,
        devices: Arc<crate::devices::Devices>,
    ) -> Result<(), Error> {
        if self.closed
            || (!config.provider_only
                && self.endpoint.borrow().is_some()
                && self.relays == config.network.relays())
        {
            return Ok(());
        }
        devices.endpoint(None).await;
        if let Some(endpoint) = self.endpoint.send_replace(None) {
            endpoint.close().await;
        }
        if config.provider_only {
            return Ok(());
        }
        let path = home.join("node.key");
        let key = crate::devices::kept(&path, 32, "Reading or making node.key")
            .await
            .map_err(other("reading the network key"))?;
        let key: SecretKey = key.parse().map_err(|_| {
            Error::Invalid("node.key is invalid; restore this home's network key.".into())
        })?;
        tokio::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .await
            .map_err(other("protecting the network key"))?;
        let transport = iroh::endpoint::QuicTransportConfig::builder()
            .max_concurrent_bidi_streams(STREAMS.into())
            .max_concurrent_uni_streams(0u32.into())
            // At most 1 MiB received and 1 MiB unacknowledged sent per connection;
            // 256 KiB received per stream shares that connection's 1 MiB cap.
            .receive_window(1_048_576u32.into())
            .stream_receive_window(262_144u32.into())
            .send_window(1_048_576)
            .max_idle_timeout(Some(iroh::endpoint::VarInt::from_u32(IDLE_MS).into()))
            .build();
        let endpoint = razor_network::open(key, config.network.relays().as_deref(), |builder| {
            builder
                .alpns(vec![
                    razor_network::API.to_vec(),
                    razor_network::PAIRING.to_vec(),
                ])
                .hooks(crate::devices::Admission(Arc::downgrade(&devices)))
                .transport_config(transport)
        })
        .await
        .map_err(other("opening the network endpoint"))?;
        devices.endpoint(Some(endpoint.clone())).await;
        self.endpoint.send_replace(Some(endpoint));
        self.relays = config.network.relays();
        Ok(())
    }
}
