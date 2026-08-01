//! The editor's pure-Rust core: document model, edit operations, lint.
//!
//! Everything here is host-testable — no Dioxus, no browser. The view layer
//! stays thin over these types.

pub mod editor_doc;
pub mod lint;
