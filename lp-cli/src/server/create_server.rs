use crate::commands::serve::init::{create_filesystem, initialize_server};
use lp_model::AsLpPath;
use lp_server::LpServer;
use lp_shared::fs::LpFs;
use lp_shared::output::MemoryOutputProvider;
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

/// Create a server instance with filesystem
///
/// # Arguments
///
/// * `dir` - Optional directory path for disk filesystem (ignored if `memory` is true)
/// * `memory` - If true, use in-memory filesystem; if false, use disk filesystem
/// * `init` - Optional initialization flag:
///   - `Some(true)`: Create `server.json` if missing
///   - `Some(false)`: Require `server.json` to exist (error if missing)
///   - `None`: Use default config (for in-memory or backward compatibility)
///
/// # Returns
///
/// * `Ok((LpServer, Box<dyn LpFs>))` if server creation succeeded
/// * `Err` if server creation failed
pub fn create_server(
    dir: Option<&Path>,
    memory: bool,
    init: Option<bool>,
) -> anyhow::Result<(LpServer, Box<dyn LpFs>)> {
    // Create filesystem
    let base_fs = create_filesystem(dir, memory)?;

    // Handle server configuration
    if memory {
        // For in-memory filesystem, use default config (no file needed)
        // Config is not actually used, but we need it for LpServer::new
    } else if let Some(init_flag) = init {
        // For disk filesystem, initialize or load config
        let server_dir = dir.unwrap_or_else(|| Path::new("../../.."));
        initialize_server(server_dir, init_flag)?;
    }
    // If init is None, use default config (for backward compatibility)

    // Create output provider
    let output_provider = Rc::new(RefCell::new(MemoryOutputProvider::new()));

    // Create LpServer (takes ownership of filesystem)
    // We need to clone the filesystem reference before passing it to LpServer
    // Since we can't clone Box<dyn LpFs>, we'll return the filesystem that was passed
    // Note: LpServer takes ownership, so we can't return the same instance
    // For now, return a new filesystem instance (caller may not need it)
    let server = LpServer::new(output_provider, base_fs, "projects/".as_path());

    // Create a new filesystem instance to return (same type as what was created)
    let returned_fs = create_filesystem(dir, memory)?;

    Ok((server, returned_fs))
}

#[cfg(test)]
mod tests {
    use crate::server::create_server;
    use lp_model::AsLpPath;
    use tempfile::TempDir;

    #[test]
    fn test_create_server_memory() {
        let (server, fs) = create_server::create_server(None, true, None).unwrap();
        // Verify server and filesystem were created
        assert!(fs.read_file("/test".as_path()).is_err()); // File doesn't exist, which is expected
        // Server should be created successfully
        drop(server);
    }

    #[test]
    fn test_create_server_disk_with_init() {
        let temp_dir = TempDir::new().unwrap();
        let dir = temp_dir.path();

        let (server, fs) = create_server::create_server(Some(dir), false, Some(true)).unwrap();
        // Verify server.json was created
        assert!(crate::config::server::server_config_exists(dir));
        // Verify filesystem works
        assert!(fs.read_file("/test".as_path()).is_err()); // File doesn't exist, which is expected
        drop(server);
    }

    #[test]
    fn test_create_server_disk_without_init_existing() {
        let temp_dir = TempDir::new().unwrap();
        let dir = temp_dir.path();

        // Create server.json first
        create_server::create_server(Some(dir), false, Some(true)).unwrap();

        // Should work with existing config
        let (server, _fs) = create_server::create_server(Some(dir), false, Some(false)).unwrap();
        drop(server);
    }

    #[test]
    fn test_create_server_disk_without_init_missing() {
        let temp_dir = TempDir::new().unwrap();
        let dir = temp_dir.path();

        // Should error when config doesn't exist and init is false
        let result = create_server::create_server(Some(dir), false, Some(false));
        assert!(result.is_err());
        if let Err(e) = result {
            let err_msg = format!("{e}");
            assert!(err_msg.contains("No server.json found"));
        }
    }

    #[tokio::test]
    async fn test_run_server_loop_async_compiles() {
        use crate::client::local::create_local_transport_pair;
        use crate::server::{create_server, run_server_loop_async};

        // Create server and transport pair
        let (server, _fs) = create_server::create_server(None, true, None).unwrap();
        let (_client_transport, server_transport) = create_local_transport_pair();

        // Verify the function can be called (it will run forever, so we don't await it)
        // The actual integration test will be in Phase 5
        let _future = run_server_loop_async::run_server_loop_async(server, server_transport);
        // Function compiles and can be created - that's what we're testing here
    }
}
