//! Errors surfaced by [`super::Engine`].

use alloc::string::String;

use crate::dataflow::binding::BindingError;
use crate::dataflow::resolver::SessionResolveError;
use crate::node::NodeError;
use crate::node::TreeError;
use lpc_model::NodeId;

/// Engine-level failures (tree, node hooks, demand resolution).
#[derive(Debug)]
pub enum EngineError {
    Tree(TreeError),
    Binding(BindingError),
    Node {
        node: NodeId,
        message: String,
    },
    Resolve(SessionResolveError),
    UnknownNode(NodeId),
    NotAlive(NodeId),
    /// Failed while flushing dirty output sinks after [`crate::engine::Engine::tick`].
    OutputFlush {
        message: String,
    },
    /// Project sync was requested while canonical sync is being rebuilt.
    ProjectSyncDisabled {
        message: String,
    },
}

impl From<TreeError> for EngineError {
    fn from(value: TreeError) -> Self {
        Self::Tree(value)
    }
}

impl From<BindingError> for EngineError {
    fn from(value: BindingError) -> Self {
        Self::Binding(value)
    }
}

impl From<SessionResolveError> for EngineError {
    fn from(value: SessionResolveError) -> Self {
        Self::Resolve(value)
    }
}

impl core::fmt::Display for EngineError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Tree(e) => write!(f, "{e:?}"),
            Self::Binding(e) => write!(f, "{e}"),
            Self::Node { node, message } => write!(f, "node {node:?}: {message}"),
            Self::Resolve(e) => write!(f, "{e}"),
            Self::UnknownNode(id) => write!(f, "unknown node {id:?}"),
            Self::NotAlive(id) => write!(f, "node {id:?} is not alive"),
            Self::OutputFlush { message } => write!(f, "output flush: {message}"),
            Self::ProjectSyncDisabled { message } => write!(f, "{message}"),
        }
    }
}

impl core::error::Error for EngineError {}

impl EngineError {
    pub(crate) fn node(node: NodeId, err: NodeError) -> Self {
        let message = match err {
            NodeError::Message(s) => s,
        };
        Self::Node { node, message }
    }
}
