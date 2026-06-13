use alloc::string::String;

/// Parent-owned placement for a child project node use.
///
/// Placement describes the authored container position that produced a child
/// use. It is model-owned so registry and engine code do not have to parse
/// project or playlist internals to understand how a child was placed.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectNodePlacement {
    /// Child from `ProjectDef.nodes[name]`.
    ProjectChild { name: String },
    /// Child from `PlaylistDef.entries[entry].node`.
    PlaylistEntry { entry: u32, name: Option<String> },
}
