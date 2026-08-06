//! Server-side records: what the service stores about a project and the
//! people who can reach it.
//!
//! These are storage records, not wire types — the client-facing vocabulary
//! lives in `lpc-cloud-api`. The one deliberate exception is
//! [`lpc_cloud_api::SidecarMeta`], which is stored verbatim because the
//! server is content-opaque (D3) and never recomputes it.

pub mod cloud_project;
pub mod cloud_user;
pub mod head_ref;
pub mod member_record;
pub mod member_role;
pub mod project_refs;
pub mod session_record;
pub mod stored_event;
