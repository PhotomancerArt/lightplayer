//! Portable project commit summaries.

use alloc::vec::Vec;

use crate::NodeKind;

use super::DefinitionLocation;

/// Portable commit summary.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ProjectCommitSummary {
    pub def_updates: ProjectDefUpdates,
    pub change_details: Vec<(DefinitionLocation, ProjectDefChangeDetail)>,
}

impl ProjectCommitSummary {
    pub fn is_empty(&self) -> bool {
        self.def_updates.is_empty() && self.change_details.is_empty()
    }
}

/// Added, changed, and removed definition locations.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ProjectDefUpdates {
    pub added: Vec<DefinitionLocation>,
    pub changed: Vec<DefinitionLocation>,
    pub removed: Vec<DefinitionLocation>,
}

impl ProjectDefUpdates {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.changed.is_empty() && self.removed.is_empty()
    }
}

/// Portable factual definition change classification.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectDefChangeDetail {
    Content,
    KindChanged { from: NodeKind, to: NodeKind },
    EnteredError,
    LeftError,
}
