//! Leased studio instances for interactive docs.
//!
//! A docs article that embeds live Studio UI declares one or two named
//! sims; each is a [`DocsSimHost`] — a **real** [`crate::StudioController`]
//! + [`crate::StudioActor`] around one browser-worker sim runtime, deployed
//! with a compiled-in example. Embeds read the host's change-gated
//! [`crate::UiStudioView`] and dispatch real [`crate::UiAction`]s, so a
//! docs knob is the same dispatch path as a Studio knob (interactive-docs
//! ADR, D1). There is no shadow "view mirror": the components cannot tell
//! docs from Studio because there is no difference to tell.
//!
//! Docs sims are **not roster sessions** (D2): they live in their own
//! controller's pool, never appear as device cards in the user's studio,
//! never hold the user's editor lens, and are torn down when the page
//! unmounts. The roster's "sim capacity stays 1" ADR governs the user's
//! pool and is untouched — each docs host has its own pool with its own
//! single sim.
//!
//! Deploys go through [`ProjectOp::OpenDocsExample`](crate::ProjectOp) →
//! `deploy_project_files`, **never** the library open path: no catalog
//! transaction, no OPFS seeding, nothing planted in the user's gallery.

mod docs_sim_host;

pub use docs_sim_host::DocsSimHost;

// Tests live with the other studio e2e rows
// (`app/studio/studio_docs_e2e_tests.rs`) so they can share the private
// in-process server harness.
