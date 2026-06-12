//! Parsed payload or error state for a node definition.
//!
//! Definition state belongs to [`crate::NodeDefEntry`]. It lets project views
//! and registry consumers keep referenced definitions in inventory even when
//! files are missing, deleted, unreadable, syntactically invalid, or
//! semantically invalid.

use alloc::string::String;

use crate::{NodeDef, NodeDefParseError, NodeKind};

/// Semantic validation failure payload.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NodeDefValidationError {
    /// Human-readable validation message.
    pub message: String,
}

impl NodeDefValidationError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// Loaded definition or structured failure for a known definition identity.
#[derive(Clone, Debug, PartialEq)]
pub enum NodeDefState {
    /// Definition parsed and validated into an authored node body.
    Loaded(NodeDef),
    /// Referenced artifact was not found.
    NotFound,
    /// Referenced artifact was deleted or is pending deletion.
    Deleted,
    /// Filesystem or artifact read failed.
    ReadError { message: String },
    /// Artifact was readable but could not be parsed as a node definition.
    ParseError(NodeDefParseError),
    /// Artifact parsed but failed semantic validation.
    ValidationError(NodeDefValidationError),
}

impl NodeDefState {
    pub fn is_loaded(&self) -> bool {
        matches!(self, Self::Loaded(_))
    }

    pub fn is_error(&self) -> bool {
        !self.is_loaded()
    }

    pub fn kind(&self) -> Option<NodeKind> {
        match self {
            Self::Loaded(def) => Some(def.kind()),
            _ => None,
        }
    }

    pub fn loaded_def(&self) -> Option<&NodeDef> {
        match self {
            Self::Loaded(def) => Some(def),
            _ => None,
        }
    }
}
