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
    NotFound,
    Deleted,
    ReadError { message: String },
    ParseError(NodeDefParseError),
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
