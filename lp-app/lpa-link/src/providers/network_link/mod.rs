//! The sans-IO half of a network link (the `browser-websocket` provider's
//! LAN half, Wi-Fi M6 P07): what a board on the LAN is called, and which key
//! a secure link presents to it.
//!
//! Declared outside the provider's wasm32 gate, like the browser worker's
//! boot-wait policy, so the policy is host-tested and Studio's core can
//! implement [`LinkKeys`] on every target.
//!
//! | file | what it owns |
//! |---|---|
//! | `lan_endpoint.rs` | the `lan:<url>` endpoint a LAN board's link wears |
//! | `lan_link_info.rs` | the model's `LinkInfo` for one (feature `device-link`) |
//! | `link_key.rs` | [`LinkKey`] (a key id and its PSK) and [`LinkKeys`], where a link's keys come from |
//! | `key_walk.rs` | [`KeyWalk`]: which key to present next after a refusal |

mod key_walk;
mod lan_endpoint;
#[cfg(feature = "device-link")]
mod lan_link_info;
mod link_key;

pub use key_walk::{BUSY_RETRY_MS, KeyRefusal, KeyWalk, KeyWalkStep};
pub use lan_endpoint::{LAN_ENDPOINT_PREFIX, lan_endpoint, url_from_lan_endpoint};
#[cfg(feature = "device-link")]
pub use lan_link_info::{lan_host, lan_link_info};
pub use link_key::{KEY_ID_BYTES, LinkKey, LinkKeys, NoLinkKeys, PSK_BYTES};
