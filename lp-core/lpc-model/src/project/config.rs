use alloc::string::String;
use serde::{Deserialize, Serialize};

/// Lightweight metadata that can travel with a project.
///
/// This is separate from the authored project node definition in
/// [`crate::ProjectDef`]. The project node defines runtime graph structure;
/// `ProjectConfig` holds user-facing metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectConfig {
    /// Stable project identifier.
    pub uid: String,
    /// Human-readable project name.
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
