//! Effective artifact reads — overlay before committed store.

use alloc::string::ToString;
use alloc::vec::Vec;

use lpfs::{LpFs, LpPath};

use crate::ArtifactId;
use crate::change::OverlayEntry;

use super::{NodeDefEntry, NodeDefId, NodeDefRegistry, NodeDefState, ParseCtx, RegistryError};
use lpc_model::{NodeDef, NodeDefParseError};

impl NodeDefRegistry {
    /// Bytes for `path` from overlay if present, else committed store/fs.
    pub fn read_effective_bytes(
        &mut self,
        path: &LpPath,
        fs: &dyn LpFs,
    ) -> Result<Option<Vec<u8>>, RegistryError> {
        if let Some(entry) = self.overlay.entry(path) {
            return Ok(match entry {
                OverlayEntry::Bytes(bytes) => Some(bytes.clone()),
                OverlayEntry::Deleted => None,
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
        if let Some(entry) = self.overlay.entry(LpPath::new(path.as_str())) {
            return Ok(match entry {
                OverlayEntry::Bytes(bytes) => parse_toml_bytes(ctx, bytes.as_slice()),
                OverlayEntry::Deleted => {
                    NodeDefState::ParseError(overlay_deleted_error(path.as_str()))
                }
            });
        }
        self.read_artifact_state(artifact_id, fs, ctx)
    }

    /// Effective state for a registered def (overlay ∪ committed cache).
    pub fn effective_state(&self, id: &NodeDefId, ctx: &ParseCtx<'_>) -> Option<NodeDefState> {
        let entry = self.entries.get(id)?;
        let path = self.artifact_root_path.get(&entry.source.artifact_id)?;
        if !self.overlay.contains_path(LpPath::new(path.as_str())) {
            return Some(entry.state.clone());
        }
        let overlay_entry = self.overlay.entry(LpPath::new(path.as_str()))?;
        Some(match overlay_entry {
            OverlayEntry::Bytes(bytes) => parse_toml_bytes(ctx, bytes.as_slice()),
            OverlayEntry::Deleted => NodeDefState::ParseError(overlay_deleted_error(path.as_str())),
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

fn overlay_deleted_error(path: &str) -> NodeDefParseError {
    NodeDefParseError::Toml {
        error: alloc::format!("artifact deleted pending commit: `{path}`"),
    }
}

pub(crate) fn read_error_state(err: crate::ArtifactError) -> NodeDefParseError {
    NodeDefParseError::Toml {
        error: alloc::format!("artifact read failed: {err:?}"),
    }
}
