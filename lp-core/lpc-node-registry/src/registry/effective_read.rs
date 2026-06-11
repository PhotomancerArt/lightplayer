//! Effective artifact reads — overlay before committed store.

use alloc::vec::Vec;

use crate::edit_apply::project_artifact_bytes;
use crate::source::{
    MaterializeError, MaterializedSource, SourceDiagnosticCtx, materialize_source,
    resolve_source_file,
};
use lpc_model::{ArtifactLocation, SourceFileSlot};
use lpc_model::{Revision, current_revision};
use lpfs::{LpFs, LpPath};

use super::effective_projection::{edit_to_registry, project_artifact_def, project_def_at_loc};
use super::{
    NodeDefEntry, NodeDefLocation, NodeDefRegistry, NodeDefState, ParseCtx, RegistryError,
};

impl NodeDefRegistry {
    /// Bytes for `path` from overlay if present, else committed store/fs.
    pub fn read_effective_bytes(
        &mut self,
        path: &LpPath,
        fs: &dyn LpFs,
        ctx: &ParseCtx<'_>,
    ) -> Result<Option<Vec<u8>>, RegistryError> {
        let committed = self.read_committed_bytes_for_path(path, fs)?;
        let pending = self
            .overlay
            .artifact(&ArtifactLocation::location_for_path(path))
            .cloned();
        project_artifact_bytes(
            committed.as_deref(),
            pending.as_ref(),
            ctx,
            current_revision(),
        )
        .map_err(edit_to_registry)
    }

    /// Parse effective TOML for an artifact (overlay ∪ base).
    pub fn parse_effective_state(
        &mut self,
        location: &crate::ArtifactLocation,
        fs: &dyn LpFs,
        ctx: &ParseCtx<'_>,
    ) -> Result<NodeDefState, RegistryError> {
        let pending = self.overlay.artifact(location).cloned();
        if pending.is_none() {
            return self.read_artifact_state(location, fs, ctx);
        }

        let committed_state = match self
            .defs
            .get(&NodeDefLocation::artifact_root(location.clone()))
        {
            Some(entry) => entry.state.clone(),
            None => self.read_artifact_state(location, fs, ctx)?,
        };

        Ok(project_artifact_def(
            &committed_state,
            pending.as_ref(),
            ctx,
        ))
    }

    /// Effective state for a registered def (overlay ∪ committed cache).
    pub fn effective_state(
        &self,
        loc: &NodeDefLocation,
        ctx: &ParseCtx<'_>,
    ) -> Option<NodeDefState> {
        let entry = self.defs.get(loc)?;
        let pending = self.overlay.artifact(&loc.artifact);
        if pending.is_none() {
            return Some(entry.state.clone());
        }
        let root_loc = NodeDefLocation::artifact_root(loc.artifact.clone());
        let root_entry = self.defs.get(&root_loc)?;
        Some(project_def_at_loc(loc, root_entry, pending, ctx))
    }

    /// Effective def entry (overlay ∪ base). Always owned.
    pub fn effective_entry(
        &self,
        loc: &NodeDefLocation,
        ctx: &ParseCtx<'_>,
    ) -> Option<NodeDefEntry> {
        let committed = self.defs.get(loc)?.clone();
        let state = self.effective_state(loc, ctx)?;
        Some(NodeDefEntry { state, ..committed })
    }

    /// Read-only effective projection over this registry.
    pub fn view(&self) -> crate::view::NodeDefView<'_> {
        crate::view::NodeDefView::new(self)
    }

    /// Materialize authored source through overlay ∪ committed store.
    pub fn materialize_source(
        &mut self,
        fs: &dyn LpFs,
        containing_file: &LpPath,
        slot: &SourceFileSlot,
        ctx: &SourceDiagnosticCtx,
        frame: Revision,
    ) -> Result<MaterializedSource, MaterializeError> {
        let reference = resolve_source_file(&mut self.store, containing_file, slot, frame)?;
        materialize_source(
            &mut self.store,
            fs,
            &reference,
            slot,
            ctx,
            Some(&self.overlay),
        )
    }

    pub(crate) fn read_committed_bytes_for_path(
        &mut self,
        path: &LpPath,
        fs: &dyn LpFs,
    ) -> Result<Option<Vec<u8>>, RegistryError> {
        let Some(location) = self.store.location_for_path(path) else {
            return Ok(None);
        };
        match self.store.read_bytes(&location, fs) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(_) => Ok(None),
        }
    }
}
