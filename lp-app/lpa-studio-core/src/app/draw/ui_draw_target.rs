//! [`UiDrawTarget`]: where a stroke's touches are written.

/// The `(scope, channel)` a drawable picture's touch set is written to:
/// the innermost scope with a `touches` reader (plan D12).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiDrawTarget {
    pub scope: lpc_wire::WireScopeRef,
    /// Always [`lpc_model::TOUCHES_CHANNEL`] today.
    pub channel: String,
}
