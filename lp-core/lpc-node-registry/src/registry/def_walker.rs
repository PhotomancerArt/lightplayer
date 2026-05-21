//! Discover nested [`NodeInvocation`] sites in parsed node definitions.

use alloc::string::String;
use alloc::vec::Vec;

use lpc_model::{ArtifactLocator, NodeDef, NodeInvocation, SlotMapKey, SlotName, SlotPath};

use super::RegistryError;

/// One authored child invocation and its path within the owning artifact.
#[derive(Clone, Debug, PartialEq)]
pub struct InvocationSite {
    pub path: SlotPath,
    pub invocation: NodeInvocation,
}

/// Collect invocation sites reachable from `def` under `base`.
pub fn collect_invocations(def: &NodeDef, base: &SlotPath) -> Vec<InvocationSite> {
    match def {
        NodeDef::Project(project) => project
            .nodes
            .entries
            .iter()
            .filter_map(|(name, invocation)| {
                Some(InvocationSite {
                    path: project_node_path(base, name)?,
                    invocation: invocation.clone(),
                })
            })
            .collect(),
        NodeDef::Playlist(playlist) => playlist
            .entries
            .entries
            .iter()
            .filter_map(|(key, entry)| {
                Some(InvocationSite {
                    path: playlist_entry_node_path(base, *key)?,
                    invocation: entry.node.clone(),
                })
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn project_node_path(base: &SlotPath, name: &str) -> Option<SlotPath> {
    let nodes = SlotName::parse("nodes").ok()?;
    let key = SlotMapKey::String(String::from(name));
    Some(base.child(nodes).child_key(key))
}

fn playlist_entry_node_path(base: &SlotPath, key: u32) -> Option<SlotPath> {
    let entries = SlotName::parse("entries").ok()?;
    let node = SlotName::parse("node").ok()?;
    Some(
        base.child(entries)
            .child_key(SlotMapKey::U32(key))
            .child(node),
    )
}

/// Resolve a path locator relative to the directory containing `containing_file`.
pub fn resolve_node_locator(
    containing_file: &lpfs::LpPath,
    locator: &ArtifactLocator,
) -> Result<lpfs::LpPathBuf, RegistryError> {
    let base_dir = containing_file
        .parent()
        .unwrap_or_else(|| lpfs::LpPath::new("/"));
    resolve_path_locator_from_dir(base_dir, locator)
}

fn resolve_path_locator_from_dir(
    base_dir: &lpfs::LpPath,
    locator: &ArtifactLocator,
) -> Result<lpfs::LpPathBuf, RegistryError> {
    match locator {
        ArtifactLocator::Path(path) => {
            if path.is_absolute() {
                Ok(path.clone())
            } else {
                base_dir
                    .to_path_buf()
                    .join_relative(path.as_str())
                    .ok_or_else(|| RegistryError::LocatorResolution {
                        message: alloc::format!(
                            "path `{}` cannot be resolved relative to `{base_dir:?}`",
                            path.as_str()
                        ),
                    })
            }
        }
        ArtifactLocator::Lib(lib) => Err(RegistryError::LocatorResolution {
            message: alloc::format!("library artifact locators are not supported: {lib}"),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;
    use lpc_model::{NodeDef, NodeDefRef};

    fn parse_def(text: &str) -> NodeDef {
        NodeDef::from_toml_str(text).expect("node def")
    }

    #[test]
    fn project_invocation_paths_use_nodes_map_keys() {
        let def = parse_def(
            r#"
kind = "Project"

[nodes.clock]
def = { path = "./clock.toml" }

[nodes.shader]
def = { path = "./shader.toml" }
"#,
        );
        let sites = collect_invocations(&def, &SlotPath::root());
        assert_eq!(sites.len(), 2);
        assert_eq!(sites[0].path.to_string(), "nodes[clock]");
        assert_eq!(sites[1].path.to_string(), "nodes[shader]");
    }

    #[test]
    fn playlist_inline_invocation_path() {
        let def = parse_def(
            r#"
kind = "Playlist"

[entries.2]
name = "active"

[entries.2.node.def]
kind = "Shader"
source = { path = "active.glsl" }
"#,
        );
        let sites = collect_invocations(&def, &SlotPath::root());
        assert_eq!(sites.len(), 1);
        assert_eq!(sites[0].path.to_string(), "entries[2].node");
        assert!(matches!(sites[0].invocation.def, NodeDefRef::Inline(_)));
    }
}
