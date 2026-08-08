//! The client side of the cloud: transport port, in-process transport, and
//! the sync engine.
//!
//! Three layers, bottom to top:
//!
//! 1. [`CloudPort`] — the transport trait, one method per plane: `call` for
//!    the control plane ([`lpc_cloud_api::CloudCall`] in,
//!    [`lpc_cloud_api::CloudReply`] out) and blob/tree get/put for the
//!    content plane. Futures are runtime-neutral (AGENTS.md sans-IO): no
//!    spawning, no executor-flavored sleeps, drivable by any edge.
//! 2. [`InProcessCloud`] — the transport that *is* the service: a real
//!    [`lp_cloud_domain::CloudService`] over the in-memory adapters, behind
//!    [`CloudPort`], answering with immediately-ready futures. It carries one
//!    session per client instance, so two clients against one
//!    [`InProcessServer`] are two users. It also has a fake offline switch
//!    ([`InProcessCloud::go_offline`]) for exercising the retry loop.
//! 3. The [`sync`] engine — [`publish`](sync::publish), [`push`](sync::push),
//!    [`pull`](sync::pull), [`apply_fast_forward`](sync::apply_fast_forward),
//!    [`resolve_clobber`](sync::resolve_clobber), and
//!    [`open_shared`](sync::open_shared), written over `lpfs::LpFs` +
//!    `lpc-history` (`SnapshotStore`, `EventLog`, `ProjectHistory`) and
//!    nothing from the studio layers.
//!
//! # What this crate does not do
//!
//! It has no retry loop, no scheduler, and no policy. Every operation is
//! **one attempt** with a typed outcome; an offline push fails with
//! [`TransportError::Offline`] and the caller decides when to try again.
//! Auto-apply is likewise a caller contract, not enforcement: see the crate
//! README for the rules (clean copies fast-forward; nothing is ever applied
//! over uncommitted edits) and for what each operation banks before it
//! adopts anything.
//!
//! # Features
//!
//! - `in-process` (default) — layer 2 above: [`InProcessCloud`] and the
//!   `lp-cloud-domain` / `lp-cloud-store-mem` halves it is built from. A
//!   browser edge that speaks HTTP turns this off
//!   (`default-features = false`) so the server does not ride along into the
//!   wasm bundle. Layers 1 and 3 — the port and the whole sync engine — do
//!   not depend on it.

#![no_std]

extern crate alloc;

pub mod block_on;
pub mod cloud_binding;
pub mod cloud_port;
#[cfg(feature = "in-process")]
pub mod in_process_cloud;
pub mod local_project;
pub mod project_link;
pub mod sync;
pub mod sync_error;

#[cfg(all(test, feature = "in-process"))]
pub(crate) mod test_support;

pub use block_on::block_on;
pub use cloud_binding::{CLOUD_BINDING_PATH, CloudBinding};
pub use cloud_port::{CloudPort, TransportError, call, request};
#[cfg(feature = "in-process")]
pub use in_process_cloud::{InProcessCloud, InProcessServer, SignedIn};
pub use local_project::LocalProject;
pub use project_link::{ProjectLink, ProjectLinkParseError};
pub use sync::{
    ApplyReport, ClobberReport, ClobberSide, OpenSharedReport, PublishReport, PullReport,
    PushReport, apply_fast_forward, open_shared, publish, pull, push, resolve_clobber,
};
pub use sync_error::SyncError;
