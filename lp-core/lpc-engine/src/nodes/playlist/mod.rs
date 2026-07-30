#[cfg(feature = "node-playlist")]
mod playlist_node;
// Always compiled — see the module doc there for why.
mod playlist_output_path;

#[cfg(feature = "node-playlist")]
pub use playlist_node::{PlaylistNode, PlaylistRuntimeEntry};
pub use playlist_output_path::playlist_output_path;
