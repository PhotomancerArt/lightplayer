//! Authored role metadata for an effective project graph node.

use alloc::string::String;

/// Parent-owned role for a child project node instance.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case", tag = "role")]
pub enum ProjectNodePlacement {
    ProjectChild { name: String },
    PlaylistEntry { entry: u32, name: Option<String> },
}
