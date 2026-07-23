//! The runtime pool: sessions the studio is attached to, plus the lens.
//!
//! Concept map (one concept per file):
//!
//! - [`runtime_id`] — [`RuntimeId`], the pool-minted session key.
//! - [`runtime_session`] — [`RuntimeSession`]: payload (sim worker or
//!   hardware [`DeviceHandle`]), per-session wire client + server state,
//!   and the device reconcile bundle.
//! - [`runtime_pool`] — [`RuntimePool`]: the keyed collection, the lens,
//!   and the two named resolution seams (lens-bound vs session-targeted).

pub mod runtime_id;
pub mod runtime_pool;
pub mod runtime_session;

pub use runtime_id::RuntimeId;
pub use runtime_pool::RuntimePool;
pub use runtime_session::{
    DeviceHandle, RuntimeKind, RuntimePayload, RuntimeSession, SimAttachment,
};
