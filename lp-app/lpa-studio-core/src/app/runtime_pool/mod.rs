//! The runtime pool: sessions the studio is attached to, plus the lens.
//!
//! Concept map (one concept per file):
//!
//! - [`runtime_id`] — [`RuntimeId`], the pool-minted session key.
//! - [`card_feed`] — [`CardFeedState`]: the session's live frame feed (the
//!   ▶ card tab's state), held by its [`RuntimeSession`].
//! - [`runtime_session`] — [`RuntimeSession`]: the [`RuntimePayload`] (sim or device lens), the
//!   per-session wire client + server state, console tail and pacing.
//! - [`runtime_op`] — [`RuntimeOp`]: the runtime-scoped verbs (stop the
//!   sim, set its log level).
//! - [`runtime_pool`] — [`RuntimePool`]: the keyed collection and the lens.
//! - [`sim_link`] — [`SimLink`]: opening the browser-worker attachment,
//!   and the `lpa-link` → UX log/error folds the sim path needs.
//!
//! The device arm returned with round-2 M5 as a LENS handle only: the
//! `lpa-devices` roster owns the device; the pool holds the editor's view of it.

pub mod card_feed;
pub mod runtime_id;
pub mod runtime_op;
pub mod runtime_pool;
pub mod runtime_session;
pub mod sim_link;

pub use card_feed::{CardFeedApply, CardFeedState};
pub use runtime_id::RuntimeId;
pub use runtime_op::RuntimeOp;
pub use runtime_pool::{RuntimePool, SIM_SESSION_CAPACITY};
pub use runtime_session::{
    CONSOLE_TAIL_LEN, DeviceLensAttachment, RuntimeKind, RuntimePayload, RuntimeSession,
    SimAttachment, SimLoadedProject,
};
pub use sim_link::SimLink;
