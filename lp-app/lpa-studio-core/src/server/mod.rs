//! Server manager model for the `lp-server` protocol session above an
//! established link.
//!
//! This layer owns handshake/status, heartbeat/log facts, loaded-project
//! discovery, and recovery/safe-mode facts. It does not own provider access,
//! flashing, raw link management, or project editing state.

pub mod server_action;
pub mod server_state;

pub use server_action::ServerActionRequest;
pub use server_state::ServerState;
