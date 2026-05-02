//! Errors surfaced by [`super::Engine`].

use alloc::string::String;

use crate::node::NodeError;
use crate::resolver::SessionResolveError;
use crate::tree::TreeError;
use lpc_model::NodeId;

/// Engine-level failures (tree, node hooks, demand resolution).
#[derive(Debug)]
pub enum EngineError {
    Tree(TreeError),
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
}

impl From<TreeError> for EngineError {
    fn from(value: TreeError) -> Self {
        Self::Tree(value)
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
            Self::Node { node, message } => write!(f, "node {node:?}: {message}"),
            Self::Resolve(e) => write!(f, "{e}"),
            Self::UnknownNode(id) => write!(f, "unknown node {id:?}"),
            Self::NotAlive(id) => write!(f, "node {id:?} is not alive"),
            Self::OutputFlush { message } => write!(f, "output flush: {message}"),
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
