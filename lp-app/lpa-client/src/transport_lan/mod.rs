//! A board on the LAN: `lan:<host>[:port]` (feature `lan`; Wi-Fi plan P06).
//!
//! A board that has joined a network serves its link at
//! `ws://<board>:80/link`: one WebSocket, one **secure** lp-link inside it
//! (Noise inside the SYN, then sealed frames; there is no plain LAN link),
//! one lp-link frame per binary message. lp-cli's `ws://` specifier already
//! means lpc-wire over a WebSocket to `lp-cli serve`, so a board gets its own
//! form, `lan:`.
//!
//! - [`LanLink`]: the link itself and how it picks its key — anonymous for
//!   an open board, a password's key for a locked one (see its docs);
//! - [`connect_lan_transport`]: the link as a [`crate::ClientTransport`] on
//!   an I/O thread ([`lan_pump`]), for `LpClient` and every lp-cli command
//!   that takes a board address;
//! - a sync caller that runs its own loop (`lp-cli link rtt|capture`) opens a
//!   [`LanLink`] and takes it apart ([`LanLink::into_parts`]).
//!
//! The host end is lp-link's `ws()` preset unchanged, selective repeat (the
//! board runs the one ARQ its other links run, cut to its own buffers, and
//! advertises its payload in its SYN). Replies are asked packed exactly when
//! the serial transports ask (`LP_WIRE_ENCODING`, [`crate::wire_encoding_env`]).
//!
//! **Known board defect, worked around here:** on the C6's server loop the
//! hello a keyed link is owed at `Up` is handed out (`take_opened_links`)
//! before `tick_and_send` takes that session's `Authenticated` event, so a
//! password session's unsolicited hello can say `granted: None` although the
//! server grants it a moment later. [`LanLink::open`] therefore asks a
//! password session's tier with a `Hello` request, answered after the grant.

mod board_password;
mod lan_entropy;
mod lan_error;
mod lan_keys;
mod lan_link;
mod lan_pump;
mod lan_socket;
mod lan_target;
mod link_endpoint;

pub use board_password::BoardPassword;
pub use lan_entropy::os_entropy;
pub use lan_error::{LOCKED_WORDS, LanError, reason_words};
pub use lan_keys::password_keys;
pub use lan_link::{BUSY_RETRIES, LAN_SETUP_BUDGET, LanLink, LanOptions, LanSession, tier_words};
pub use lan_socket::LanSocket;
pub use lan_target::{LAN_DEFAULT_PORT, LAN_LINK_PATH, LanTarget};
pub use link_endpoint::{LAN_BUSY_CLOSE, LinkEndpoint};

use std::sync::Arc;
use std::sync::atomic::AtomicU32;

use lpc_wire::ServerHello;
use tokio::sync::{mpsc, oneshot};

use crate::transport_serial::AsyncSerialClientTransport;
use lan_pump::LanPump;

/// Open `target` — a board on the LAN ([`LanTarget::endpoint`]) or through
/// the relay ([`crate::transport_relay::RelayTarget::endpoint`]) — its
/// secure session up, its tier known, and serve it as a client transport.
/// Returns the session's hello beside it.
pub async fn connect_lan_transport(
    target: LinkEndpoint,
    options: LanOptions,
) -> Result<(AsyncSerialClientTransport, ServerHello), LanError> {
    let (client_tx, client_rx) = mpsc::unbounded_channel();
    let (server_tx, server_rx) = mpsc::unbounded_channel();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let (opened_tx, opened_rx) = oneshot::channel();
    let link_generation = Arc::new(AtomicU32::new(0));
    let label = target.to_string();
    let pump = LanPump {
        target,
        options,
        client_rx,
        server_tx,
        shutdown_rx,
        link_generation: Arc::clone(&link_generation),
        opened: opened_tx,
    };
    let thread = std::thread::Builder::new()
        .name(format!("lan-link {label}"))
        .spawn(move || pump.run())
        .map_err(|error| LanError::Lost(format!("no thread for the link: {error}")))?;
    let mut transport = AsyncSerialClientTransport::new(
        client_tx,
        server_rx,
        link_generation,
        shutdown_tx,
        thread,
        label,
    );
    let opened = opened_rx
        .await
        .unwrap_or_else(|_| Err(LanError::Lost("the link thread ended".to_string())));
    match opened {
        Ok(hello) => Ok((transport, hello)),
        Err(error) => {
            let _ = crate::ClientTransport::close(&mut transport).await;
            Err(error)
        }
    }
}
