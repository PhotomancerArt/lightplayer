//! [`AgentSessionKey`]: identity of one per-shader agent conversation.

use crate::RuntimeId;

/// Key of one agent chat session: the runtime session the editor lens was
/// bound to when the conversation started, plus the shader node's stable
/// address. Sessions are in-memory for the app lifetime (no persistence —
/// plan decision Q7); a different runtime session (reopen, other project)
/// starts a fresh conversation.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct AgentSessionKey {
    /// The pool session the conversation belongs to.
    pub runtime: RuntimeId,
    /// The shader node's stable address (`ProjectNodeAddress` display form).
    pub node: String,
}

impl AgentSessionKey {
    pub fn new(runtime: RuntimeId, node: impl Into<String>) -> Self {
        Self {
            runtime,
            node: node.into(),
        }
    }
}
