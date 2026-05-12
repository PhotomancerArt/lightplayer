use alloc::string::String;
use serde::{Deserialize, Serialize};

/// Project configuration - minimal, no nodes field
///
/// Nodes are discovered from filesystem, not stored in config.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectConfig {
    pub uid: String,
    pub name: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    #[test]
    fn test_project_config_creation() {
        let config = ProjectConfig {
            uid: "test-uid".to_string(),
            name: "Test Project".to_string(),
        };
        assert_eq!(config.uid, "test-uid");
        assert_eq!(config.name, "Test Project");
    }
}
