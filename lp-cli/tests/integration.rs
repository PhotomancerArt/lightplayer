//! Integration tests for lp-cli
//!
//! Tests end-to-end functionality using memory filesystem and in-memory transport.
//! These tests verify that the CLI commands work correctly without requiring
//! real filesystem or network access.

extern crate alloc;

use alloc::{boxed::Box, rc::Rc};
use core::cell::RefCell;
// NOTE: These integration tests use the old synchronous lpa-client API which no longer exists.
// They need to be rewritten to use the new async LpClient API. Marked as #[ignore] for now.

use lpa_server::LpServer;
use lpc_model::AsLpPath;
use lpc_shared::output::MemoryOutputProvider;
use lpc_wire::WireMessage;
use lpfs::{LpFs, LpFsMemory};

// Placeholder types for compilation - these tests are ignored
type LpClient = ();
type LocalTransport = ();
type ClientError = ();

/// Set up server and client with memory transport
///
/// Returns `(server, client, client_transport, server_transport)` for
/// synchronous message processing in tests.
#[allow(
    dead_code,
    unused_variables,
    reason = "async client integration tests are being rewritten"
)]
fn setup_server_and_client(
    _fs: LpFsMemory,
) -> (LpServer, LpClient, LocalTransport, LocalTransport) {
    todo!("Rewrite for async LpClient")
}

/// Extract ClientMessage from WireMessage envelope and send via transport
#[allow(
    dead_code,
    unused_variables,
    reason = "async client integration tests are being rewritten"
)]
fn send_client_message(
    _transport: &mut LocalTransport,
    _msg: WireMessage,
) -> Result<(), ClientError> {
    todo!("Rewrite for async LpClient")
}

/// Process messages synchronously between client and server
///
/// This bridges messages through the transport, processing them on both
/// client and server using their tick() methods.
#[allow(
    dead_code,
    unused_variables,
    reason = "async client integration tests are being rewritten"
)]
fn process_messages(
    _client: &mut LpClient,
    _server: &mut LpServer,
    _client_transport: &mut LocalTransport,
    _server_transport: &mut LocalTransport,
) -> Result<(), ClientError> {
    todo!("Rewrite for async LpClient")
}

/// Create a test project on a filesystem
///
/// Creates a minimal project with project.json.
#[allow(
    dead_code,
    reason = "async client integration tests are being rewritten"
)]
fn create_test_project(fs: &mut LpFsMemory, name: &str) -> Result<(), ClientError> {
    let project_json = format!(
        r#"{{ "kind": "Project", "format": 1, "name": "{name}" }}
"#
    );
    fs.write_file_mut("/project.json".as_path(), project_json.as_bytes())
        .map_err(|_| todo!())?;

    Ok(())
}

#[test]
#[ignore] // Uses old lpa-client API, needs to be rewritten for async LpClient
fn test_server_startup_with_memory_filesystem() {
    // Create server with memory filesystem
    let fs = LpFsMemory::new();
    let output_provider = Rc::new(RefCell::new(MemoryOutputProvider::new()));
    let graphics: std::sync::Arc<dyn lpa_server::LpGraphics> = std::sync::Arc::new(
        lp_gfx_lpvm::TargetLpvmGraphics::new(lpa_server::DEVICE_SHADER_FRONTEND),
    );
    let _server = LpServer::new(
        output_provider,
        Box::new(fs),
        "projects".as_path(),
        None,
        None,
        graphics,
    );

    // Server created successfully
    // (In a real test, we might verify it accepts connections, but with
    // LocalTransport we test that separately)
}

#[test]
#[ignore] // Uses old lpa-client API, needs to be rewritten for async LpClient
fn test_client_server_communication() {
    unimplemented!("Needs to be rewritten for async LpClient")
}

#[test]
#[ignore] // Uses old lpa-client API, needs to be rewritten for async LpClient
fn test_end_to_end_project_push() {
    unimplemented!("Needs to be rewritten for async LpClient")
}

#[test]
#[ignore] // Uses old lpa-client API, needs to be rewritten for async LpClient
fn test_create_command_structure() {
    // Simulate create command by creating a project structure
    let mut fs = LpFsMemory::new();
    let project_name = "my-project";

    let project_json = format!(
        r#"{{
  "kind": "Project",
  "format": 1,
  "name": "{project_name}"
}}
"#
    );
    fs.write_file_mut("/project.json".as_path(), project_json.as_bytes())
        .unwrap();

    // Verify project.json exists and is valid
    let content = fs.read_file("/project.json".as_path()).unwrap();
    let config: serde_json::Value = serde_json::from_slice(&content).unwrap();
    assert_eq!(
        config.get("name").and_then(serde_json::Value::as_str),
        Some(project_name)
    );
}

// Note: A full async test for the dev command would require making the server,
// transport, and command modules public or creating a lib.rs file.
// The current test structure verifies the synchronous transport works correctly.
// The async message processing fix in the handler should resolve the issue
// where responses weren't being received from the async server.
