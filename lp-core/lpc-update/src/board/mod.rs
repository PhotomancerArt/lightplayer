//! **The board's update session**: a sans-IO state machine the firmware
//! drives (Part B) and host tests drive over an in-memory NOR model
//! ([`crate::testing`], feature `test-support`).
//!
//! The session is [`BoardSession`]; what it needs from the firmware is
//! [`UpdateTarget`] (flash, placement, the split image's format hooks) and
//! [`BoardFacts`] (read once at start). Who may do what is
//! [`access_rule`]; the core-side login is [`core_login`].

pub mod access_rule;
pub mod board_link;
pub mod board_session;
pub mod core_login;
pub mod manifest_view;
pub mod piece_hash;
pub mod piece_stage;
pub mod read_back;
pub mod running_engine;
pub mod session_output;
pub mod transfer;
pub mod transfer_owner;
pub mod transfer_plan;
pub mod update_target;
pub mod update_window;

pub use access_rule::{AccessFacts, CORE_INSTALL_FOLLOWS_OPEN_TO, Operation, may};
pub use board_link::{LinkId, LinkTrust};
pub use board_session::BoardSession;
pub use session_output::{Effect, OWNER_QUIET_MS, Outgoing, SessionConfig};
pub use update_target::{BoardFacts, EngineStatus, FlashFault, SessionMode, UpdateTarget};
