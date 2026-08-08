//! The Studio's edge onto the cloud service.
//!
//! Three small pieces, and no UI (P5/P6 render what this holds):
//!
//! - [`fetch_cloud_port`] — the browser's `CloudPort`: `fetch` over the two
//!   deployed planes, same-origin so the session cookie rides along.
//! - [`session_state`] — one `Signal<CloudSession>` in context, fed once at
//!   boot and re-fed on demand: pending → anonymous / signed-in /
//!   unreachable.
//! - [`account_memory`] — the `lp_accounts` localStorage list the
//!   switch-account rows are built from. Pure list logic, host-tested.

pub mod account_memory;
pub mod fetch_cloud_port;
pub mod session_state;

pub use fetch_cloud_port::FetchCloudPort;
pub use session_state::{CloudSession, CloudSessionRefresh, use_cloud_session_provider};
