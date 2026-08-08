//! The cloud sync service's domain logic, behind injected ports.
//!
//! [`CloudService`] answers every [`lpc_cloud_api::CloudRequest`] against a
//! [`MetaStore`], a [`Clock`], and an [`IdMint`]. It is sans-IO (AGENTS.md):
//! no executor, no ambient clock, no randomness — timestamps arrive through
//! the clock port and random bytes through the mint port, mirroring
//! `lpc-history`'s caller-supplied-randomness posture. Transport, cookies,
//! and blob *bytes* belong to the server edge; the domain sees only hashes.
//!
//! # The rules that live here
//!
//! - **Access ⊥ membership.** [`Access`](lpc_cloud_api::Access) says what
//!   holding the *link* grants — nothing (`None`), reading (`View`), or
//!   reading and writing (`Edit`) — and membership is the orthogonal
//!   per-account grant that always allows both. A project the caller can
//!   reach neither way answers `NotFound`, never `NotAuthorized`: existence
//!   itself must not leak.
//! - **Archiving is not deleting.** An archived project stops resolving for
//!   everyone but its members and refuses every write; its owner can restore
//!   it, and nothing is thrown away.
//! - **Push is never blocked** (D5). A push that does not continue the
//!   server's line becomes an *additional head*; the client resolves the
//!   divergence later with a clobber join, which collapses the frontier back
//!   to one head. The only push refusals are unknown project, no write
//!   access, an archived project, missing blobs, and a malformed event batch
//!   — see [`push_validation`].
//! - **The server is content-opaque** (D3). It never opens a tree manifest
//!   and never derives display metadata; [`lpc_cloud_api::SidecarMeta`] is
//!   stored verbatim exactly as the client computed it.
//! - **The client owns project uids** (D21). `PublishProject` records the
//!   uid it was handed; the service mints only user uids and session tokens.
//!
//! # Ports
//!
//! [`MetaStore`] is deliberately **one** trait: users, sessions, projects,
//! membership, refs, sidecars, the event log, and the blob index are one
//! consistency domain (one SQLite transaction, once P04 lands). [`BlobStore`]
//! is separate because blob bytes live on a different plane entirely (an
//! object store), and the domain never touches them.

#![no_std]

extern crate alloc;

pub mod cloud_service;
pub mod model;
pub mod ports;
pub mod push_validation;

pub use cloud_service::CloudService;
pub use lpc_cloud_api::MemberRole;
pub use model::caller::Caller;
pub use model::cloud_project::CloudProject;
pub use model::cloud_user::CloudUser;
pub use model::head_ref::HeadRef;
pub use model::login_providers::{DevPickerConnection, LoginProviders, OidcConnection};
pub use model::member_record::MemberRecord;
pub use model::project_refs::ProjectRefs;
pub use model::session_record::{SESSION_TOKEN_LEN, SessionRecord, session_token_hash};
pub use model::stored_event::StoredEvent;
pub use ports::blob_store::BlobStore;
pub use ports::clock::Clock;
pub use ports::id_mint::IdMint;
pub use ports::meta_store::MetaStore;
pub use push_validation::{PushValidation, validate_push_events};
