//! ResolveError — focused error type for slot resolution failures.

use alloc::format;
use alloc::string::String;

/// Error during slot resolution in the binding cascade.
///
/// Carries a descriptive message for debugging; no structured error
/// taxonomy in M4.3.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolveError {
    pub message: String,
}

impl ResolveError {
    /// Create a new resolve error with a message.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    /// Create an error for node-prop that doesn't target outputs namespace.
    pub fn node_prop_not_outputs(actual_namespace: impl Into<String>) -> Self {
        Self::new(format!(
            "NodeProp binding must target outputs namespace, got: {}",
            actual_namespace.into()
        ))
    }

    /// Create an error for a missing target node in NodeProp resolution.
    pub fn target_node_not_found(node_path: impl Into<String>) -> Self {
        Self::new(format!(
            "NodeProp target node not found: {}",
            node_path.into()
        ))
    }

    /// Create an error for a missing property on a target node.
    pub fn target_prop_not_found(
        node_path: impl Into<String>,
        prop_path: impl Into<String>,
    ) -> Self {
        Self::new(format!(
            "NodeProp property not found on target {}: {}",
            node_path.into(),
            prop_path.into()
        ))
    }

    /// Create an error for unresolvable binding.
    pub fn unresolvable(prop_path: impl Into<String>) -> Self {
        Self::new(format!(
            "Could not resolve binding for property: {}",
            prop_path.into()
        ))
    }
}

impl core::fmt::Display for ResolveError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl core::error::Error for ResolveError {}

#[cfg(test)]
mod tests {
    use super::ResolveError;

    #[test]
    fn resolve_error_new_stores_message() {
        let err = ResolveError::new("test message");
        assert_eq!(err.message, "test message");
    }

    #[test]
    fn node_prop_not_outputs_formats_correctly() {
        let err = ResolveError::node_prop_not_outputs("params");
        assert!(err.message.contains("NodeProp binding must target outputs"));
        assert!(err.message.contains("params"));
    }

    #[test]
    fn target_node_not_found_formats_correctly() {
        let err = ResolveError::target_node_not_found("/show/node1");
        assert!(err.message.contains("target node not found"));
        assert!(err.message.contains("/show/node1"));
    }

    #[test]
    fn target_prop_not_found_formats_correctly() {
        let err = ResolveError::target_prop_not_found("/show/node1", "outputs.color");
        assert!(err.message.contains("NodeProp property not found"));
        assert!(err.message.contains("/show/node1"));
        assert!(err.message.contains("outputs.color"));
    }

    #[test]
    fn unresolvable_formats_correctly() {
        let err = ResolveError::unresolvable("params.speed");
        assert!(err.message.contains("Could not resolve"));
        assert!(err.message.contains("params.speed"));
    }

    #[test]
    fn display_trait_works() {
        let err = ResolveError::new("test");
        let s = alloc::format!("{}", err);
        assert_eq!(s, "test");
    }
}
