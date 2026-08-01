//! The board display-def editor: a standalone, project-agnostic editor for
//! `boards/<vendor>/<product>.display.json` sidecars.
//!
//! Boundary (mapping-editor precedent, plan decision D1): document in, edits
//! out. This crate knows the display sidecar schema ([`lpa_boards`]) and how
//! to edit it — it has **no** knowledge of projects, devices, routes, or the
//! Studio server. The host (`#/boards/edit`) owns persistence and mounts the
//! editor.
//!
//! `editor_core` is pure Rust — the document model, edit operations, and the
//! lint rules are host-testable with no browser or Dioxus involvement. View
//! components (Dioxus) stay thin over the core, and the live preview reuses
//! [`lpa_boards::BoardDiagram`] unchanged — the renderer is a dependency
//! here, never something this crate redraws.

pub mod editor_core;
pub mod view;

pub use editor_core::editor_doc::{EditorDoc, RailTarget, canonical_json};
pub use editor_core::lint::{LintFinding, LintLevel, lint_board};
pub use view::board_editor_page::BoardEditorPage;
