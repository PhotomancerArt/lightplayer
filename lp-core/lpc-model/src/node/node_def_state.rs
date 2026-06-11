//! Parsed payload or error state for a node definition.

use alloc::string::String;

use crate::{NodeDef, NodeDefParseError, NodeKind};

/// Semantic validation failure payload.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NodeDefValidationError {
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
    Loaded(NodeDef),
    ParseError(NodeDefParseError),
    ValidationError(NodeDefValidationError),
}

impl NodeDefState {
    pub fn is_loaded(&self) -> bool {
        matches!(self, Self::Loaded(_))
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
