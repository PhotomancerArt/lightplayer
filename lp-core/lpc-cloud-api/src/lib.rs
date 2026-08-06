//! Client↔cloud-service message vocabulary.
//!
//! This crate is pure Rust types: the entire request/response/error
//! vocabulary a LightPlayer client and the cloud sync service exchange, plus
//! the version-and-refuse envelope that carries [`CLOUD_API_VERSION`]. There
//! is no transport, no IO, and no logic beyond the version-refusal helper in
//! [`version`] — see the crate README for the full policy statement and the
//! fw-graph-clean rule (nothing in `lp-fw` may ever depend on this crate).
//!
//! Concept-per-file: each module below owns one vocabulary concept.

#![no_std]
extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

pub mod actor;
pub mod envelope;
pub mod error;
pub mod head_info;
pub mod project_meta;
pub mod request;
pub mod response;
pub mod sidecar_meta;
pub mod version;
pub mod visibility;

pub use actor::Actor;
pub use envelope::{CloudCall, CloudReply};
pub use error::CloudError;
pub use head_info::{HeadInfo, PushOutcome};
pub use project_meta::ProjectMeta;
pub use request::CloudRequest;
pub use response::CloudResponse;
pub use sidecar_meta::SidecarMeta;
pub use version::{CLOUD_API_VERSION, check_version};
pub use visibility::Visibility;
