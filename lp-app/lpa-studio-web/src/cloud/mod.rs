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
//! - [`sync`] — auto-publish: the driver that keeps a signed-in account's
//!   projects on the service without anyone pressing anything.
//! - [`shared_open`] — somebody else's `/p/` link becoming a tracking copy
//!   in the library (P6), or the calm not-found line on Home.

pub mod account_memory;
pub mod fetch_cloud_port;
pub mod session_state;
pub mod shared_open;
pub mod sync;

pub use fetch_cloud_port::FetchCloudPort;
pub use session_state::{CloudSession, CloudSessionRefresh, use_cloud_session_provider};
pub use shared_open::SharedOpenState;

/// Ensure this browser holds a session — minting a GUEST account when it
/// has none (examples vision D3/D8: an anonymous fork's publish needs an
/// owner, and sign-in must not gate it). `POST /auth/guest` is idempotent
/// by cookie: a live session (guest or real) mints nothing. Returns
/// whether the call landed; the caller refreshes [`CloudSession`] after.
#[cfg(target_arch = "wasm32")]
pub async fn ensure_guest_session() -> bool {
    match gloo_net::http::Request::post("/auth/guest").send().await {
        Ok(response) if response.ok() => true,
        Ok(response) => {
            log::warn!("guest session mint answered {}", response.status());
            false
        }
        Err(error) => {
            log::warn!("guest session mint failed: {error}");
            false
        }
    }
}
