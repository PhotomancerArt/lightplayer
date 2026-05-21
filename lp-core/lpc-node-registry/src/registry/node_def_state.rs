//! Parsed payload or error state for a registry entry.

use lpc_model::{NodeDef, NodeDefParseError, NodeKind};

/// Reserved placeholder for semantic validation failures (unused in M2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidationErrorPlaceholder {
    message: alloc::string::String,
}

impl ValidationErrorPlaceholder {
    pub fn new(message: impl Into<alloc::string::String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// Loaded definition or structured failure for a known def identity.
#[derive(Clone, Debug, PartialEq)]
pub enum NodeDefState {
    Loaded(NodeDef),
    ParseError(NodeDefParseError),
    ValidationError(ValidationErrorPlaceholder),
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
