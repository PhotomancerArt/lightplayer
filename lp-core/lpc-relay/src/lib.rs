//! The cloud relay's device-leg protocol, shared by every party to it: the
//! hub in `lp-cloud-server`, the board's firmware, lp-cli's host board, and
//! the tests.
//!
//! The relay is a pipe. A board on Wi-Fi holds **one** WebSocket to
//! `ws://lightplayer.app/relay/device` (the device leg); each browser session
//! is its own WebSocket to `/relay/board/<mac>` (the browser leg) carrying
//! bare lp-link frames. The hub joins the two by a **route id**, and every
//! lp-link frame crosses the device leg wrapped in a [`RelayFrame::Frame`].
//! The lp-link session inside is Noise-sealed end to end: the hub can read
//! none of it.
//!
//! What this crate holds:
//!
//! - **The framing** ([`RelayFrame`]): a one-byte tag and fixed
//!   little-endian fields, length-checked, never serde, at most
//!   [`MAX_RELAY_FRAME`] bytes. One WebSocket binary message is one frame.
//! - **The board's hello** ([`RelayHello`]): its MAC, name, wire version,
//!   LAN address, the salts of the account keys it holds, and (protocol 2)
//!   its firmware version.
//! - **What the board reports** (protocol 2): its project ([`RelayProject`],
//!   the name in the clear and the uid and content hash only as keyed tags,
//!   [`relay_project`]) and its picture ([`RelayPicture`]), at the cadence
//!   the hub asks ([`PictureRate`]).
//! - **The proof** ([`relay_proof`]): how a board shows it holds an
//!   account's key without sending it.
//! - **The version** ([`RELAY_PROTO_VERSION`]): version-and-refuse, like the
//!   cloud API, because fielded boards outlive cloud deploys. The hub
//!   accepts protocols 1 and 2, and never sends a board a frame of a later
//!   protocol than its own ([`RelayFrame::protocol`]).
//! - **The board's client** ([`relay_client::RelayClient`]): a sans-IO state
//!   machine that decides when to dial, backs off, answers the challenge and
//!   keeps the route table. The firmware and lp-cli drive the same one.
//!
//! Sans-IO throughout: time is a caller-supplied millisecond count, random
//! bytes come from a caller-supplied `fn(&mut [u8])`, and nothing here opens
//! a socket, resolves a name or reads a file. Decision record:
//! `docs/adr/2026-10-06-cloud-relay.md`.

#![no_std]
extern crate alloc;
#[cfg(any(test, feature = "std"))]
extern crate std;

mod frame_reader;
pub mod lan_address;
pub mod picture_rate;
pub mod refuse_reason;
pub mod relay_board_id;
pub mod relay_client;
pub mod relay_close_code;
pub mod relay_frame;
pub mod relay_hello;
pub mod relay_limits;
pub mod relay_picture;
pub mod relay_project;
pub mod relay_proof;
pub mod relay_version;
pub mod route_close_reason;

pub use lan_address::LanAddress;
pub use picture_rate::PictureRate;
pub use refuse_reason::RefuseReason;
pub use relay_board_id::{BadRelayBoardId, RelayBoardId};
pub use relay_client::{
    RelayAccount, RelayAction, RelayClient, RelayClientConfig, RelayEvent, RelayState,
};
pub use relay_close_code::RelayCloseCode;
pub use relay_frame::{
    ROUTE_FRAME_OVERHEAD, RelayFrame, RelayFrameError, encode_route_frame, frame_protocol,
    route_frame_header,
};
pub use relay_hello::RelayHello;
pub use relay_limits::{
    DEFAULT_PICTURE_SAMPLES, MAX_FIRMWARE_BYTES, MAX_HELLO_ACCOUNTS, MAX_IDLE_S, MAX_LABEL_BYTES,
    MAX_PICTURE_OUTPUTS, MAX_PROJECT_NAME_BYTES, MAX_RELAY_FRAME, MAX_ROUTES_PER_BOARD,
    MAX_WATCHED_FOR_S, MIN_IDLE_S, MIN_WATCHED_MS, PING_INTERVAL_S, PROJECT_TAG_BYTES,
    SILENT_CLOSE_S,
};
pub use relay_picture::RelayPicture;
pub use relay_project::{
    RELAY_PROJECT_LABEL, RelayProject, project_content_tag, project_tag_key, project_uid_tag,
};
pub use relay_proof::{
    RELAY_AUTH_LABEL, RELAY_NONCE_BYTES, RELAY_PROOF_BYTES, relay_auth_key, relay_proof,
    verify_relay_proof,
};
pub use relay_version::{
    RELAY_DEVICE_PATH, RELAY_PROTO_1, RELAY_PROTO_2, RELAY_PROTO_VERSION,
    SUPPORTED_RELAY_PROTO_VERSIONS, check_relay_version,
};
pub use route_close_reason::RouteCloseReason;
