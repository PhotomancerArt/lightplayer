//! Server configuration handling
//!
//! Functions for loading and saving server.json configuration files.

use anyhow::{Context, Result};
use std::path::Path;

use crate::config::ServerConfig;

/// Check if server.json exists in the given directory
pub fn server_config_exists(dir: &Path) -> bool {
    dir.join("server.json").exists()
}

/// Load server configuration from server.json
///
/// # Arguments
///
/// * `dir` - Server directory containing server.json
///
/// # Returns
///
/// * `Ok(ServerConfig)` if the file exists and is valid
/// * `Err` if the file doesn't exist or cannot be parsed
pub fn load_server_config(dir: &Path) -> Result<ServerConfig> {
    let config_path = dir.join("server.json");
    let contents = std::fs::read_to_string(&config_path)
        .with_context(|| format!("Failed to read server.json from {}", dir.display()))?;

    let config: ServerConfig = serde_json::from_str(&contents)
        .with_context(|| format!("Failed to parse server.json from {}", dir.display()))?;

    Ok(config)
}

/// Save server configuration to server.json
///
/// # Arguments
///
/// * `dir` - Server directory where server.json should be written
/// * `config` - Server configuration to save
///
/// # Returns
///
/// * `Ok(())` if the file was written successfully
/// * `Err` if the file cannot be written
pub fn save_server_config(dir: &Path, config: &ServerConfig) -> Result<()> {
    // Ensure directory exists
    std::fs::create_dir_all(dir)
        .with_context(|| format!("Failed to create server directory: {}", dir.display()))?;

    let config_path = dir.join("server.json");
    let json =
        serde_json::to_string_pretty(config).context("Failed to serialize server configuration")?;

    std::fs::write(&config_path, json)
        .with_context(|| format!("Failed to write server.json to {}", config_path.display()))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_server_config_exists() {
        let temp_dir = TempDir::new().unwrap();
        let dir = temp_dir.path();

        // Initially doesn't exist
        assert!(!server_config_exists(dir));

        // Create server.json
        let config = ServerConfig::default();
        save_server_config(dir, &config).unwrap();

        // Now exists
        assert!(server_config_exists(dir));
    }

    #[test]
    fn test_load_server_config() {
        let temp_dir = TempDir::new().unwrap();
        let dir = temp_dir.path();

        // Create server.json
        let config = ServerConfig::default();
        save_server_config(dir, &config).unwrap();

        // Load it back
        let loaded = load_server_config(dir).unwrap();
        assert_eq!(loaded, config);
    }

    #[test]
    fn test_save_server_config() {
        let temp_dir = TempDir::new().unwrap();
        let dir = temp_dir.path();

        let config = ServerConfig::default();
        save_server_config(dir, &config).unwrap();

        // Verify file exists and has correct content
        let config_path = dir.join("server.json");
        assert!(config_path.exists());

        let contents = fs::read_to_string(&config_path).unwrap();
        assert_eq!(contents.trim(), "{}");
    }

    #[test]
    fn test_load_nonexistent_config() {
        let temp_dir = TempDir::new().unwrap();
        let dir = temp_dir.path();

        let result = load_server_config(dir);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Failed to read server.json")
        );
    }
}
