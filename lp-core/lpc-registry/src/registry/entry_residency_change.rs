//! Changing which playlist entries are loaded.
//!
//! [`ProjectRegistry::set_entry_resident`] and
//! [`ProjectRegistry::make_only_resident`] update the registry's
//! [`EntryResidency`] and re-derive, returning the same
//! [`ProjectChangeSummary`] an edit returns: a newly resident entry's subtree
//! shows up as added uses/defs/assets, a newly dormant one's as removed. The
//! engine applies that summary exactly like an edit's.
//!
//! A dormant entry leaves nothing behind: its tree nodes, def and asset rows
//! drop out of the re-derived inventory, and its artifact-store locations are
//! unregistered by [`ProjectRegistry::release_left_behind`].

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use lpc_model::{
    ArtifactLocation, NodeDefLocation, NodeUseLocation, PlaylistDef, ProjectChangeSummary,
    ProjectInventory, Revision, resolve_artifact_specifier,
};
use lpfs::{LpFs, LpPathBuf};

use crate::overlay::inventory_change_summary::change_summary_between;
use crate::{EntryResidency, EntryResidencyError, ParseCtx, ProjectRegistry};

impl ProjectRegistry {
    /// The current residency set.
    pub fn residency(&self) -> &EntryResidency {
        &self.residency
    }

    /// Load (`resident = true`) or unload one entry of the playlist used at
    /// `playlist`, and re-derive.
    ///
    /// Returns an empty summary when the entry already had that residency.
    /// Refuses, changing nothing, when the playlist or entry is unknown or
    /// when unloading would drop an artifact that carries pending slot edits
    /// ([`EntryResidencyError::PendingEdits`]).
    pub fn set_entry_resident(
        &mut self,
        fs: &dyn LpFs,
        playlist: &NodeUseLocation,
        entry: u32,
        resident: bool,
        frame: Revision,
        ctx: &ParseCtx<'_>,
    ) -> Result<ProjectChangeSummary, EntryResidencyError> {
        let (playlist_artifact, idle_entry) = self.resident_playlist(playlist, entry)?;
        let mut next = self.residency.clone();
        if !next.set_resident(playlist, entry, resident, idle_entry) {
            return Ok(ProjectChangeSummary::default());
        }
        self.apply_residency(fs, next, playlist_artifact, entry, frame, ctx)
    }

    /// Make `entry` the only loaded entry of the playlist used at `playlist`
    /// (load it, unload every other), and re-derive. Same contract as
    /// [`Self::set_entry_resident`].
    pub fn make_only_resident(
        &mut self,
        fs: &dyn LpFs,
        playlist: &NodeUseLocation,
        entry: u32,
        frame: Revision,
        ctx: &ParseCtx<'_>,
    ) -> Result<ProjectChangeSummary, EntryResidencyError> {
        let (playlist_artifact, idle_entry) = self.resident_playlist(playlist, entry)?;
        let mut next = self.residency.clone();
        if !next.make_only_resident(playlist, entry, idle_entry) {
            return Ok(ProjectChangeSummary::default());
        }
        self.apply_residency(fs, next, playlist_artifact, entry, frame, ctx)
    }

    /// Load every entry of every playlist — including playlists inside the
    /// entries this loads — and re-derive. Returns the summary across the
    /// whole operation.
    ///
    /// For host tooling that must see every authored file: gates that compile
    /// or validate each shader, Studio's all-entries check (vision D19). A
    /// device holds one entry per playlist and never calls this.
    pub fn make_every_entry_resident(
        &mut self,
        fs: &dyn LpFs,
        frame: Revision,
        ctx: &ParseCtx<'_>,
    ) -> ProjectChangeSummary {
        let before = self.inventory.clone();
        loop {
            let mut changed = false;
            for (use_location, node) in &self.inventory.tree.nodes {
                let Some(playlist) = self
                    .inventory
                    .defs
                    .get(&node.def_location)
                    .and_then(|entry| entry.state.loaded_def())
                    .and_then(|def| def.as_playlist())
                else {
                    continue;
                };
                let idle_entry = *playlist.idle_entry.value();
                for key in playlist.entries.entries.keys() {
                    changed |= self
                        .residency
                        .set_resident(use_location, *key, true, idle_entry);
                }
            }
            if !changed {
                break;
            }
            // Loading only adds; nothing leaves, so no pending edit can be
            // stranded.
            let after = self.derive_inventory(fs, frame, ctx);
            self.inventory = after;
        }
        change_summary_between(&before, &self.inventory)
    }

    /// The dormant playlist entry an artifact outside the inventory belongs
    /// to, if one can be named without loading it.
    ///
    /// Matches the entry's own def file (its resolved `ref`). When the entry
    /// def lives in a directory of its own (not the playlist's), any file
    /// under that directory matches too — the layout of imported patterns
    /// (`modules/<name>/…`). Files of a dormant entry that share the
    /// playlist's directory and are not its def cannot be attributed without
    /// parsing it, and are reported as unknown.
    pub(crate) fn dormant_entry_owning(&self, artifact: &ArtifactLocation) -> Option<DormantEntry> {
        let target = artifact.file_path();
        for (use_location, node) in &self.inventory.tree.nodes {
            let Some(playlist) = self
                .inventory
                .defs
                .get(&node.def_location)
                .and_then(|entry| entry.state.loaded_def())
                .and_then(|def| def.as_playlist())
            else {
                continue;
            };
            let playlist_file = node.def_location.artifact.file_path();
            let idle_entry = *playlist.idle_entry.value();
            for (key, entry) in &playlist.entries.entries {
                if self.residency.is_resident(use_location, *key, idle_entry) {
                    continue;
                }
                let Some(entry_file) = entry
                    .node
                    .value()
                    .ref_specifier()
                    .and_then(|spec| resolve_artifact_specifier(playlist_file, &spec).ok())
                else {
                    continue;
                };
                if entry_owns_file(playlist_file, &entry_file, target) {
                    return Some(DormantEntry {
                        playlist: node.def_location.artifact.clone(),
                        entry: *key,
                        name: entry.name.data.as_ref().map(|name| name.value().clone()),
                    });
                }
            }
        }
        None
    }

    /// Validate a residency target: the use is a loaded playlist that has
    /// `entry`. Returns the playlist's artifact and effective idle entry.
    fn resident_playlist(
        &self,
        playlist: &NodeUseLocation,
        entry: u32,
    ) -> Result<(ArtifactLocation, u32), EntryResidencyError> {
        let node = self
            .inventory
            .tree
            .nodes
            .get(playlist)
            .ok_or(EntryResidencyError::UnknownPlaylist)?;
        let artifact = node.def_location.artifact.clone();
        let def: Option<&PlaylistDef> = self
            .inventory
            .defs
            .get(&node.def_location)
            .and_then(|def_entry| def_entry.state.loaded_def())
            .and_then(|def| def.as_playlist());
        let Some(def) = def else {
            return Err(EntryResidencyError::NotAPlaylist { def: artifact });
        };
        if !def.entries.entries.contains_key(&entry) {
            return Err(EntryResidencyError::UnknownEntry {
                playlist: artifact,
                entry,
            });
        }
        Ok((artifact, *def.idle_entry.value()))
    }

    /// Install `next`, re-derive, and commit the result — or restore the
    /// previous set and refuse when an artifact with pending slot edits would
    /// leave the inventory (a later commit could not write it).
    fn apply_residency(
        &mut self,
        fs: &dyn LpFs,
        next: EntryResidency,
        playlist: ArtifactLocation,
        entry: u32,
        frame: Revision,
        ctx: &ParseCtx<'_>,
    ) -> Result<ProjectChangeSummary, EntryResidencyError> {
        let previous = core::mem::replace(&mut self.residency, next);
        let after = self.derive_inventory_unreleased(fs, frame, ctx);

        let stranded = self.stranded_slot_edits(&after);
        if !stranded.is_empty() {
            self.residency = previous;
            self.release_registered_by(&after);
            return Err(EntryResidencyError::PendingEdits {
                playlist,
                entry,
                artifacts: stranded,
            });
        }

        self.release_left_behind(&after);
        let changes = change_summary_between(&self.inventory, &after);
        self.inventory = after;
        Ok(changes)
    }

    /// Def artifacts carrying pending slot edits that are in the current
    /// inventory and not in `after`.
    fn stranded_slot_edits(&self, after: &ProjectInventory) -> Vec<ArtifactLocation> {
        self.overlay
            .get()
            .iter()
            .filter(|(_, overlay)| overlay.as_slot().is_some())
            .map(|(location, _)| location)
            .filter(|location| {
                let def = NodeDefLocation::artifact_root((*location).clone());
                self.inventory.defs.contains_key(&def) && !after.defs.contains_key(&def)
            })
            .cloned()
            .collect()
    }

    /// Undo the registrations a refused derivation made: locations `after`
    /// walks that the current inventory does not hold and no overlay covers.
    fn release_registered_by(&mut self, after: &ProjectInventory) {
        let mut fresh: Vec<ArtifactLocation> = Vec::new();
        for def in after.defs.keys() {
            if !self.inventory.defs.contains_key(def) {
                fresh.push(def.artifact.clone());
            }
        }
        for source in after.assets.keys() {
            if !self.inventory.assets.contains_key(source) {
                let lpc_model::AssetLocation::Artifact { location } = source;
                fresh.push(location.clone());
            }
        }
        for location in fresh {
            let still_held = self
                .inventory
                .defs
                .contains_key(&NodeDefLocation::artifact_root(location.clone()))
                || self
                    .inventory
                    .assets
                    .contains_key(&lpc_model::AssetLocation::artifact(location.clone()));
            if !still_held && !self.overlay.get().contains_artifact(&location) {
                let _ = self.artifacts.unregister(&location);
            }
        }
    }
}

/// A dormant playlist entry, named for a rejection message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DormantEntry {
    pub playlist: ArtifactLocation,
    pub entry: u32,
    pub name: Option<String>,
}

impl DormantEntry {
    /// "entry 2 ("blast") of playlist /playlist.json is not loaded; load it
    /// to edit it".
    pub fn not_loaded_message(&self) -> String {
        let name = match &self.name {
            Some(name) => format!(" (\"{name}\")"),
            None => String::new(),
        };
        format!(
            "entry {}{name} of playlist {} is not loaded; load it to edit it",
            self.entry,
            self.playlist.file_path()
        )
    }
}

/// Whether `target` belongs to the entry whose def is `entry_file`: it is
/// that def, or it sits under the entry's own directory — one that is not
/// the playlist's directory or an ancestor of it.
fn entry_owns_file(
    playlist_file: &lpfs::LpPath,
    entry_file: &LpPathBuf,
    target: &LpPathBuf,
) -> bool {
    if target == entry_file {
        return true;
    }
    let (Some(entry_dir), Some(playlist_dir)) =
        (entry_file.as_path().parent(), playlist_file.parent())
    else {
        return false;
    };
    !playlist_dir.starts_with(entry_dir.as_str()) && target.starts_with(entry_dir.as_str())
}
