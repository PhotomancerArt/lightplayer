//! Errors returned from [`super::NodeRuntime`] lifecycle hooks.

use alloc::string::String;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NodeError {
    Message(String),
}

impl NodeError {
    pub fn msg(text: impl Into<String>) -> Self {
        Self::Message(text.into())
    }
}

/// Wrap any displayable error with static call-site context
/// (`"{context}: {error}"`).
///
/// Shared `map_err` adapter so the many graphics/product call sites in node
/// code reuse one formatting path per error type instead of monomorphizing a
/// `format!` each (code-size matters on firmware).
pub(crate) fn err_ctx<E: core::fmt::Display>(context: &'static str) -> impl FnOnce(E) -> NodeError {
    move |error| NodeError::Message(alloc::format!("{context}: {error}"))
}

impl core::fmt::Display for NodeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Message(message) => f.write_str(message),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_error_msg_stores_message() {
        let err = NodeError::msg("hello");
        assert_eq!(err, NodeError::Message(String::from("hello")));
    }
}
