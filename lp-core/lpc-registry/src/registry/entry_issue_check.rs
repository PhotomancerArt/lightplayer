//! Per-entry problem reports across every entry of every playlist (D19).
//!
//! [`ProjectRegistry::entry_issues`] is a *report*, not a mutation: call it
//! against a registry already loaded with every entry resident (see
//! [`ProjectRegistry::make_every_entry_resident`]). A device loads only the
//! playing entry, so a broken dormant pattern would otherwise only be found
//! when it is picked. Studio and `lp-cli upload` both have every file on the
//! host, so they check them all: on edit (Studio, a warning — a
//! work-in-progress must still save) and on upload (`lp-cli`, a refusal —
//! the same "refuse where they refuse, warn where they warn" split the
//! runtime already uses for a broken node).

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use lp_collection::VecMap;
use lpc_model::{ArtifactLocation, NodeDefState, NodeUseLocation, ProjectNodePlacement};

use crate::ProjectRegistry;

/// One playlist entry whose def failed to load, named the way
/// `DormantEntry::not_loaded_message` names a dormant one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntryIssue {
    /// The owning playlist's own artifact.
    pub playlist: ArtifactLocation,
    /// The entry's key within `PlaylistDef.entries`.
    pub entry: u32,
    /// The entry's authored display name, if it has one.
    pub name: Option<String>,
    /// What went wrong, from the def's [`NodeDefState`].
    pub message: String,
}

impl EntryIssue {
    /// "entry 2 ("blast") of playlist /playlist.json: <problem>" — the same
    /// shape as `DormantEntry::not_loaded_message`, so the two read as one
    /// family of message in logs and notices.
    pub fn display(&self) -> String {
        let name = match &self.name {
            Some(name) => format!(" (\"{name}\")"),
            None => String::new(),
        };
        format!(
            "entry {}{name} of playlist {}: {}",
            self.entry,
            self.playlist.file_path(),
            self.message
        )
    }
}

impl ProjectRegistry {
    /// Every entry, of every playlist, whose def is not [`NodeDefState::Loaded`]
    /// in the *current* inventory.
    ///
    /// Only meaningful once every entry is resident — a dormant entry has no
    /// tree node at all (AC1) and is silently absent here, not reported as
    /// broken. Callers that want the all-entries check load with
    /// [`Self::make_every_entry_resident`] first.
    ///
    /// A broken node deep inside an entry's subtree (not the entry's own
    /// top-level def) is attributed to the entry whose invocation chain
    /// reaches it, by walking `ProjectNode::parent` up from the broken use
    /// to the nearest entry root.
    pub fn entry_issues(&self) -> Vec<EntryIssue> {
        let inventory = &self.inventory;

        // Every playlist-entry root use, keyed by its own `NodeUseLocation`,
        // so a broken descendant can be walked back up to the entry that
        // owns it.
        let mut entry_roots: VecMap<NodeUseLocation, (ArtifactLocation, u32, Option<String>)> =
            VecMap::default();
        for (use_location, node) in inventory.tree.nodes.iter() {
            let Some(def) = inventory
                .defs
                .get(&node.def_location)
                .and_then(|entry| entry.state.loaded_def())
            else {
                continue;
            };
            if def.as_playlist().is_none() {
                continue;
            }
            for site in def.invocation_sites() {
                let ProjectNodePlacement::PlaylistEntry { entry, name } = site.role else {
                    continue;
                };
                let child_key = use_location.child(site.path);
                entry_roots.insert(child_key, (node.def_location.artifact.clone(), entry, name));
            }
        }

        let mut issues = Vec::new();
        for (use_location, node) in inventory.tree.nodes.iter() {
            let Some(def_entry) = inventory.defs.get(&node.def_location) else {
                continue;
            };
            if def_entry.state.is_loaded() {
                continue;
            }
            let Some((playlist, entry, name)) =
                owning_entry(&entry_roots, &inventory.tree, use_location)
            else {
                continue;
            };
            issues.push(EntryIssue {
                playlist,
                entry,
                name,
                message: describe_def_state(&def_entry.state),
            });
        }
        issues
    }
}

/// Walk `ProjectNode::parent` up from `start` to the nearest entry root
/// `entry_roots` names, if any.
fn owning_entry(
    entry_roots: &VecMap<NodeUseLocation, (ArtifactLocation, u32, Option<String>)>,
    tree: &lpc_model::ProjectTree,
    start: &NodeUseLocation,
) -> Option<(ArtifactLocation, u32, Option<String>)> {
    let mut cursor = Some(start.clone());
    while let Some(key) = cursor {
        if let Some(owner) = entry_roots.get(&key) {
            return Some(owner.clone());
        }
        cursor = tree.nodes.get(&key).and_then(|node| node.parent.clone());
    }
    None
}

fn describe_def_state(state: &NodeDefState) -> String {
    match state {
        NodeDefState::Loaded(_) => String::from("loaded"),
        NodeDefState::NotFound => String::from("referenced file was not found"),
        NodeDefState::Deleted => String::from("referenced file is deleted"),
        NodeDefState::ReadError { message } => format!("could not be read: {message}"),
        NodeDefState::ParseError(err) => format!("could not be parsed: {err}"),
        NodeDefState::ValidationError(err) => format!("failed validation: {}", err.message),
    }
}
