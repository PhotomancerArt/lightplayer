//! Errors returned from [`super::Node`] lifecycle hooks.

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_error_msg_stores_message() {
        let err = NodeError::msg("hello");
        assert_eq!(err, NodeError::Message(String::from("hello")));
    }
}
