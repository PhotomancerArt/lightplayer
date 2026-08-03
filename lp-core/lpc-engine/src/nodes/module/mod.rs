//! Module runtime node — the bus-scope output mirror (modules.md R7).
//!
//! Deliberately NOT feature-gated: every project has a root module, so the
//! runtime links into every firmware build including the C6 minimal image.

mod module_mirror_state;
mod module_node;

pub use module_mirror_state::ModuleMirrorState;
pub use module_node::ModuleNode;
