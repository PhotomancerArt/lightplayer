//! The runtime pool: the session the studio is attached to, plus the lens.
//!
//! Concept map (one concept per file):
//!
//! - [`runtime_id`] — [`RuntimeId`], the pool-minted session key.
//! - [`runtime_session`] — [`RuntimeSession`]: the [`RuntimePayload`] (one
//!   arm — a device lens), the per-session wire client + server state,
//!   console tail and pacing.
//! - [`runtime_op`] — [`RuntimeOp`]: the runtime-scoped verbs (open/close
//!   the lens, set the runtime's log level).
//! - [`runtime_pool`] — [`RuntimePool`]: the keyed collection and the lens.
//!
//! Every runtime is a roster device (PD9): the `lpa-devices` roster owns the
//! device — identity, evidence, activities, link — and the pool holds only
//! the editor's view of it. A sim is a device like any other; the one thing
//! left that distinguishes them here is [`LinkTransport`], a fact about the
//! wire.

pub mod runtime_id;
pub mod runtime_op;
pub mod runtime_pool;
pub mod runtime_session;

pub use runtime_id::RuntimeId;
pub use runtime_op::RuntimeOp;
pub use runtime_pool::{RuntimePool, SESSION_CAPACITY};
pub use runtime_session::{
    CONSOLE_TAIL_LEN, DeviceLensAttachment, LinkTransport, RuntimePayload, RuntimeSession,
};
