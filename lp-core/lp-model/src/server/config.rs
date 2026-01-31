//! Server configuration
//!
//! Defines the structure for server.json configuration files.
//! Currently minimal, but designed to be extended with server-level settings.

use serde::{Deserialize, Serialize};

/// Server configuration
///
/// This struct represents the configuration for a LightPlayer server instance.
/// It is stored in `server.json` at the root of the server directory.
///
/// Currently empty, but will be extended with fields such as:
/// - `memory_limits`: Memory usage limits for projects
/// - `security_rules`: Security and access control rules
/// - `projects_dir`: Custom projects directory path (defaults to "projects/")
/// - `logs_dir`: Custom logs directory path
/// - `port`: Custom websocket port (defaults to 2812)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerConfig {
    // Future fields will be added here as needed
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_config_serialization() {
        let config = ServerConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        assert_eq!(json, "{}");
    }

    #[test]
    fn test_server_config_deserialization() {
        let json = "{}";
        let config: ServerConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config, ServerConfig::default());
    }

    #[test]
    fn test_server_config_default() {
        let config = ServerConfig::default();
        assert_eq!(config, ServerConfig {});
    }
}
