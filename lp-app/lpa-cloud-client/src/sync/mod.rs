//! The sync engine: one file per operation.
//!
//! Every operation is one attempt over a [`CloudPort`](crate::CloudPort),
//! against a [`LocalProject`](crate::LocalProject). None of them loop, wait,
//! or decide policy. The order they compose in is the product's own:
//!
//! ```text
//! publish ──► push ◄──────────────┐
//!                                 │
//! open_shared ──► pull ──► apply_fast_forward
//!                   └────► resolve_clobber ──► push
//! ```

pub mod apply;
pub mod open_shared;
pub mod publish;
pub mod pull;
pub mod push;
pub mod resolve_clobber;

mod commit_parents;
mod content_transfer;
mod event_diff;
mod service_calls;

pub use apply::{ApplyReport, apply_fast_forward};
pub use open_shared::{OpenSharedReport, open_shared};
pub use publish::{PublishReport, publish};
pub use pull::{PullReport, pull};
pub use push::{PushReport, push};
pub use resolve_clobber::{ClobberReport, ClobberSide, resolve_clobber};
