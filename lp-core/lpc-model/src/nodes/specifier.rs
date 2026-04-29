use alloc::string::{String, ToString};
use serde::{Deserialize, Serialize};

/// Node specifier - currently just a path string
///
/// May support other specifier types in the future (e.g., expressions, handles).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeSpecifier(pub String);

impl NodeSpecifier {
    pub fn new(spec: String) -> Self {
        Self(spec)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for NodeSpecifier {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for NodeSpecifier {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_specifier_creation() {
        let spec = NodeSpecifier::new("/src/test.texture".to_string());
        assert_eq!(spec.as_str(), "/src/test.texture");
    }

    #[test]
    fn test_node_specifier_from_string() {
        let spec = NodeSpecifier::from("/src/test.shader".to_string());
        assert_eq!(spec.as_str(), "/src/test.shader");
    }

    #[test]
    fn test_node_specifier_from_str() {
        let spec = NodeSpecifier::from("/src/test.output");
        assert_eq!(spec.as_str(), "/src/test.output");
    }
}
