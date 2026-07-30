//! The playlist node's `output` slot path.
//!
//! Split out of `playlist_node` (rather than gated behind `node-playlist`
//! with the rest of that module) because `project_loader::register_node_bindings`
//! calls it unconditionally for every projected `NodeDef::Playlist` — that
//! match arm is model-level binding registration and must keep working even
//! when the `PlaylistNode` runtime itself is compiled out.

use lpc_model::SlotPath;

pub fn playlist_output_path() -> SlotPath {
    SlotPath::parse("output").expect("playlist output path")
}
