//! The board's end of the device leg: when to dial, how to back off, the
//! challenge, and the route table. Sans-IO; the firmware's relay task and
//! lp-cli's host board drive the same [`RelayClient`].

mod relay_account;
mod relay_action;
mod relay_backoff;
#[allow(
    clippy::module_inception,
    reason = "search-friendly file names: the client lives in relay_client.rs"
)]
mod relay_client;
mod relay_client_config;
mod relay_event;
mod relay_routes;
mod relay_state;

pub use relay_account::RelayAccount;
pub use relay_action::RelayAction;
pub use relay_backoff::{
    FIRST_BACKOFF_MS, GOING_AWAY_MAX_MS, GOING_AWAY_MIN_MS, MAX_BACKOFF_MS, RelayBackoff,
};
pub use relay_client::{
    CONNECT_TIMEOUT_MS, HANDSHAKE_TIMEOUT_MS, RESOLVE_TIMEOUT_MS, RelayClient,
    VERSION_REFUSED_RETRY_MS,
};
pub use relay_client_config::RelayClientConfig;
pub use relay_event::RelayEvent;
pub use relay_routes::RelayRoutes;
pub use relay_state::RelayState;
