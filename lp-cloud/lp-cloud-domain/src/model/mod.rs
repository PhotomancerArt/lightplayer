//! Server-side records: what the service stores about a project and the
//! people who can reach it.
//!
//! These are storage records, not wire types — the client-facing vocabulary
//! lives in `lpc-cloud-api`. There are two deliberate exceptions, both
//! re-exported rather than re-declared so there is one spelling of each
//! concept: [`lpc_cloud_api::SidecarMeta`], stored verbatim because the
//! server is content-opaque (D3) and never recomputes it, and
//! [`lpc_cloud_api::MemberRole`], which travels on the wire inside
//! [`lpc_cloud_api::MemberInfo`] and is stored as-is on a
//! [`member_record::MemberRecord`].

pub mod caller;
pub mod cloud_project;
pub mod cloud_user;
pub mod head_ref;
pub mod login_providers;
pub mod member_record;
pub mod project_refs;
pub mod session_record;
pub mod stored_event;
