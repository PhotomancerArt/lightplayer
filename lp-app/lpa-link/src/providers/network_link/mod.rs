//! The sans-IO half of a network link (the `browser-websocket` provider,
//! Wi-Fi M6 P07 and the network transport's relay): what a board on the LAN
//! or through lightplayer.app's relay is called, and which key a secure link
//! presents to it.
//!
//! Declared outside the provider's wasm32 gate, like the browser worker's
//! boot-wait policy, so the policy is host-tested and Studio's core can
//! implement [`LinkKeys`] on every target.
//!
//! | file | what it owns |
//! |---|---|
//! | `lan_endpoint.rs` | the `lan:<url>` endpoint a LAN board's link wears |
//! | `lan_link_info.rs` | the model's `LinkInfo` for one (feature `device-link`) |
//! | `lan_name_fallback.rs` | the board's `lp-xxxx.local` socket, tried when its IP stops answering |
//! | `relay_endpoint.rs` | the `relay:<board>` endpoint, and the relay browser leg (`wss://<host>/relay/board/<board>`) that reaches it |
//! | `relay_link_info.rs` | the model's `LinkInfo` for one (feature `device-link`) |
//! | `link_key.rs` | [`LinkKey`] (a key id and its PSK) and [`LinkKeys`], where a link's keys come from |
//! | `key_walk.rs` | [`KeyWalk`]: which key to present next after a refusal |

mod key_walk;
mod lan_endpoint;
#[cfg(feature = "device-link")]
mod lan_link_info;
mod lan_name_fallback;
mod link_key;
mod relay_endpoint;
#[cfg(feature = "device-link")]
mod relay_link_info;

pub use key_walk::{BUSY_RETRY_MS, KeyRefusal, KeyWalk, KeyWalkStep};
pub use lan_endpoint::{LAN_ENDPOINT_PREFIX, lan_endpoint, url_from_lan_endpoint};
#[cfg(feature = "device-link")]
pub use lan_link_info::{lan_host, lan_link_info};
pub use lan_name_fallback::lan_name_fallback;
pub use link_key::{KEY_ID_BYTES, LinkKey, LinkKeys, NoLinkKeys, PSK_BYTES};
pub use relay_endpoint::{
    RELAY_ENDPOINT_PREFIX, RELAY_SOCKET_PATH, board_from_relay_endpoint,
    board_from_relay_socket_url, relay_endpoint, relay_host, relay_socket_url,
};
#[cfg(feature = "device-link")]
pub use relay_link_info::relay_link_info;
