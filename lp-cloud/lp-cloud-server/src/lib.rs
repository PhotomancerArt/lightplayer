//! The cloud service's HTTP edge.
//!
//! One crate, three planes (D11/D26), all on one origin:
//!
//! - **Control** — `POST /api`: a [`lpc_cloud_api::CloudCall`] in, a
//!   [`lpc_cloud_api::CloudReply`] out. Version-refusal and cookie→[`Actor`]
//!   resolution happen here; everything past them is
//!   [`CloudService::handle`](lp_cloud_domain::CloudService::handle).
//! - **Content** — `GET`/`PUT /b/{hash}` for file blobs and
//!   `GET`/`PUT /t/{hash}` for tree manifests. Two route pairs rather than
//!   one because a tree is addressed by its *package* hash (see
//!   [`content::tree_route`]), which is not the hash of the bytes that carry
//!   it.
//! - **Page** — `GET /p/{share}` (the share URL, with OG tags injected when
//!   the project is link-visible), the static app artifact, and an SPA
//!   fallback for every path that is not a file.
//!
//! # Everything blocking goes through `spawn_blocking`
//!
//! [`CloudService`](lp_cloud_domain::CloudService) is sans-IO and entirely
//! **synchronous**, and so are both stores behind it. The SQLite adapter
//! blocks on a file, and `S3BlobStore` owns a private current-thread tokio
//! runtime whose `block_on` *panics* if it is called from a thread already
//! running a runtime. So the service and the blob store live together behind
//! one [`std::sync::Mutex`] ([`AppState::with_service`]), and every handler
//! reaches them through [`tokio::task::spawn_blocking`] — uniformly, whether
//! the configured backend actually blocks or not. A handler that locks the
//! mutex directly is a bug waiting for the S3 backend to be switched on; the
//! rule is easier to keep than the exception is to audit.
//!
//! # What the edge owns
//!
//! Cookies, transport, hash verification of uploaded bytes, static files,
//! and OG injection. Not access rules, not visibility, not push validation —
//! those are `lp-cloud-domain`'s, and this crate never re-implements or
//! re-tests them.
//!
//! [`Actor`]: lpc_cloud_api::Actor

pub mod api;
pub mod app_state;
pub mod auth;
pub mod config;
pub mod content;
pub mod page;
pub mod ports;
pub mod router;

pub use app_state::{AppState, ServiceCore};
pub use config::{BlobBackend, ConfigError, MetaBackend, ServerConfig};
pub use router::build_router;
