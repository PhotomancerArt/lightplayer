//! Server initialization logic
//!
//! Functions for initializing server directory and filesystem.

use anyhow::{Context, Result};
use std::path::Path;

use crate::config::ServerConfig;
use crate::config::server::{load_server_config, save_server_config, server_config_exists};
use crate::messages::{format_command, print_error_and_return};
use lp_shared::fs::{LpFs, LpFsMemory, LpFsStd};

/// Initialize server directory
///
/// Checks for server.json and creates it if `--init` flag is set.
/// Returns an error if server.json is missing and `--init` is not set.
pub fn initialize_server(dir: &Path, init: bool) -> Result<ServerConfig> {
    if !server_config_exists(dir) {
        if init {
            // Create server.json with default config
            let config = ServerConfig::default();
            save_server_config(dir, &config)
                .with_context(|| format!("Failed to create server.json in {}", dir.display()))?;
            Ok(config)
        } else {
            // Error: server.json missing and --init not provided
            let cmd = format_command(&format!("lp-cli serve {} --init", dir.display()));
            return Err(print_error_and_return(
                &format!("No server.json found in {}", dir.display()),
                &[&format!("To initialize a new server, run: {cmd}")],
            ));
        }
    } else {
        // Load existing server.json
        load_server_config(dir)
            .with_context(|| format!("Failed to load server.json from {}", dir.display()))
    }
}

/// Create filesystem for server
///
/// Returns either a memory filesystem or a standard filesystem based on the `--memory` flag.
/// This function accepts parameters for testability.
pub fn create_filesystem(dir: Option<&Path>, memory: bool) -> Result<Box<dyn LpFs>> {
    if memory {
        // LpFsMemory now uses interior mutability, so it can implement write_file() directly
        Ok(Box::new(LpFsMemory::new()))
    } else {
        let server_dir = dir.unwrap_or_else(|| Path::new("../../../.."));
        Ok(Box::new(LpFsStd::new(server_dir.to_path_buf())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lp_model::AsLpPath;
    use tempfile::TempDir;

    #[test]
    fn test_initialize_server_with_init() {
        let temp_dir = TempDir::new().unwrap();
        let dir = temp_dir.path();

        // Should create server.json when --init is true
        let config = initialize_server(dir, true).unwrap();
        assert!(server_config_exists(dir));
        assert_eq!(config, ServerConfig::default());
    }

    #[test]
    fn test_initialize_server_without_init() {
        let temp_dir = TempDir::new().unwrap();
        let dir = temp_dir.path();

        // Should error when --init is false and server.json doesn't exist
        let result = initialize_server(dir, false);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("No server.json found")
        );
    }

    #[test]
    fn test_initialize_server_existing() {
        let temp_dir = TempDir::new().unwrap();
        let dir = temp_dir.path();

        // Create server.json first
        let config1 = initialize_server(dir, true).unwrap();

        // Should load existing server.json
        let config2 = initialize_server(dir, false).unwrap();
        assert_eq!(config1, config2);
    }

    #[test]
    fn test_create_filesystem_memory() {
        let fs = create_filesystem(None, true).unwrap();
        // Memory filesystem should work
        assert!(fs.read_file("/test".as_path()).is_err()); // File doesn't exist, which is expected
    }

    #[test]
    fn test_create_filesystem_std() {
        let temp_dir = TempDir::new().unwrap();
        let dir = temp_dir.path();

        let fs = create_filesystem(Some(dir), false).unwrap();
        // Standard filesystem should work
        assert!(fs.read_file("/test".as_path()).is_err()); // File doesn't exist, which is expected
    }

    #[test]
    fn test_create_filesystem_std_default_dir() {
        let fs = create_filesystem(None, false).unwrap();
        // Should default to current directory
        assert!(fs.read_file("/test".as_path()).is_err()); // File doesn't exist, which is expected
    }
}
