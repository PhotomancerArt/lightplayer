//! Serving a build: the session that answers a board's requests.

pub mod serve_session;

pub use serve_session::{
    BLE_READ_BACK_PIECE, ServeConfig, ServeCounters, ServeEvent, ServeOutput, ServeSession,
};
