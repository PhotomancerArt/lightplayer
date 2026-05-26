//! Effective artifact reads — overlay before committed store.

use alloc::string::ToString;
use alloc::vec::Vec;

use lpfs::{LpFs, LpPath};

use super::slot_apply::serialize_slot_draft;
use crate::ArtifactId;
use crate::edit::SlotOverlayEntry;
use crate::source::{
    MaterializeError, MaterializedSource, SourceDiagnosticCtx, materialize_source,
    resolve_source_file,
};
use lpc_model::{NodeDef, NodeDefParseError, NodeInvocation, Revision, SlotPath, SourceFileSlot};

use super::{NodeDefEntry, NodeDefId, NodeDefRegistry, NodeDefState, ParseCtx, RegistryError};
use crate::registry::def_walker::collect_invocations;

impl NodeDefRegistry {
    /// Bytes for `path` from overlay if present, else committed store/fs.
    pub fn read_effective_bytes(
        &mut self,
        path: &LpPath,
        fs: &dyn LpFs,
        ctx: &ParseCtx<'_>,
    ) -> Result<Option<Vec<u8>>, RegistryError> {
        if let Some(entry) = self.slot_overlay.entry(path) {
            return Ok(match entry {
                SlotOverlayEntry::Bytes(bytes) => Some(bytes.clone()),
                SlotOverlayEntry::DefDraft(draft) => {
                    Some(serialize_slot_draft(&draft.def, ctx).map_err(|err| {
                        RegistryError::InvalidPath {
                            message: err.to_string(),
                        }
                    })?)
                }
                SlotOverlayEntry::Deleted => None,
            });
        }
        let Some(id) = self.artifact_path_to_id.get(path.as_str()).copied() else {
            return Ok(None);
        };
        match self.store.read_bytes(&id, fs) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(_) => Ok(None),
        }
    }

    /// Parse effective TOML for an artifact (overlay ∪ base).
    pub fn parse_effective_state(
        &mut self,
        artifact_id: ArtifactId,
        fs: &dyn LpFs,
        ctx: &ParseCtx<'_>,
    ) -> Result<NodeDefState, RegistryError> {
        let path = self
            .artifact_root_path
            .get(&artifact_id)
            .ok_or(RegistryError::UnknownDef)?;
        if let Some(entry) = self.slot_overlay.entry(LpPath::new(path.as_str())) {
            return Ok(match entry {
                SlotOverlayEntry::Bytes(bytes) => effective_state_from_slot_overlay_bytes(
                    bytes.as_slice(),
                    &SlotPath::root(),
                    ctx,
                    &NodeDefState::ParseError(slot_overlay_deleted_error(path.as_str())),
                ),
                SlotOverlayEntry::DefDraft(draft) => {
                    def_state_at_source(&draft.def, &SlotPath::root()).unwrap_or_else(|| {
                        NodeDefState::ParseError(slot_overlay_deleted_error(path.as_str()))
                    })
                }
                SlotOverlayEntry::Deleted => {
                    NodeDefState::ParseError(slot_overlay_deleted_error(path.as_str()))
                }
            });
        }
        self.read_artifact_state(artifact_id, fs, ctx)
    }

    /// Effective state for a registered def (overlay ∪ committed cache).
    pub fn effective_state(&self, id: &NodeDefId, ctx: &ParseCtx<'_>) -> Option<NodeDefState> {
        let entry = self.entries.get(id)?;
        let path = self.artifact_root_path.get(&entry.loc.artifact_id)?;
        if !self.slot_overlay.contains_path(LpPath::new(path.as_str())) {
            return Some(entry.state.clone());
        }
        let overlay_entry = self.slot_overlay.entry(LpPath::new(path.as_str()))?;
        Some(match overlay_entry {
            SlotOverlayEntry::Bytes(bytes) => effective_state_from_slot_overlay_bytes(
                bytes.as_slice(),
                &entry.loc.path,
                ctx,
                &entry.state,
            ),
            SlotOverlayEntry::DefDraft(draft) => {
                def_state_at_source(&draft.def, &entry.loc.path)
                    .unwrap_or_else(|| entry.state.clone())
            }
            SlotOverlayEntry::Deleted => {
                NodeDefState::ParseError(slot_overlay_deleted_error(path.as_str()))
            }
        })
    }

    /// Effective def entry (overlay ∪ base). Always owned.
    pub fn effective_entry(&self, id: &NodeDefId, ctx: &ParseCtx<'_>) -> Option<NodeDefEntry> {
        let committed = self.entries.get(id)?.clone();
        let state = self.effective_state(id, ctx)?;
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
            Some(&self.slot_overlay),
        )
    }
}

pub(crate) fn parse_toml_bytes(ctx: &ParseCtx<'_>, bytes: &[u8]) -> NodeDefState {
    let text = match core::str::from_utf8(bytes) {
        Ok(text) => text,
        Err(err) => {
            return NodeDefState::ParseError(NodeDefParseError::Toml {
                error: err.to_string(),
            });
        }
    };
    match NodeDef::read_toml(ctx.shapes, text) {
        Ok(def) => NodeDefState::Loaded(def),
        Err(err) => NodeDefState::ParseError(err),
    }
}

fn slot_overlay_deleted_error(path: &str) -> NodeDefParseError {
    NodeDefParseError::Toml {
        error: alloc::format!("artifact deleted pending commit: `{path}`"),
    }
}

fn effective_state_from_slot_overlay_bytes(
    bytes: &[u8],
    source_path: &lpc_model::SlotPath,
    ctx: &ParseCtx<'_>,
    fallback: &NodeDefState,
) -> NodeDefState {
    match parse_toml_bytes(ctx, bytes) {
        NodeDefState::Loaded(root) => {
            def_state_at_source(&root, source_path).unwrap_or(fallback.clone())
        }
        other => other,
    }
}

fn def_state_at_source(root: &NodeDef, source_path: &lpc_model::SlotPath) -> Option<NodeDefState> {
    if source_path.is_root() {
        return Some(NodeDefState::Loaded(root.clone()));
    }
    for site in collect_invocations(root, &lpc_model::SlotPath::root()) {
        if site.path == *source_path {
            return match &site.invocation {
                NodeInvocation::Unset | NodeInvocation::Ref(_) => None,
                NodeInvocation::Def(body) => Some(NodeDefState::Loaded(body.value().clone())),
            };
        }
    }
    None
}

pub(crate) fn read_error_state(err: crate::ArtifactError) -> NodeDefParseError {
    NodeDefParseError::Toml {
        error: alloc::format!("artifact read failed: {err:?}"),
    }
}
