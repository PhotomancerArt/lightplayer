//! Serve command handler
//!
//! Orchestrates the serve command execution.

use anyhow::Result;
use std::path::PathBuf;

use super::args::ServeArgs;
use super::server_loop::run_server_loop;
use crate::server::create_server;
use crate::server::transport_ws::WebSocketServerTransport;

/// Handle the serve command
///
/// Initializes server, creates filesystem, starts LpServer, and runs the main loop.
pub fn handle_serve(args: ServeArgs) -> Result<()> {
    // Determine server directory (default to current directory)
    let server_dir = args.dir.unwrap_or_else(|| PathBuf::from("../../../.."));

    // Create server using shared function
    let (server, _base_fs) =
        create_server::create_server(Some(&server_dir), args.memory, Some(args.init))?;

    // Create websocket server transport on port 2812
    let transport = WebSocketServerTransport::new(2812)
        .map_err(|e| anyhow::anyhow!("Failed to start websocket server: {e}"))?;

    println!("Server started on ws://localhost:2812/");
    println!("Press Ctrl+C to stop");

    // Run server loop (blocks until interrupted)
    run_server_loop(server, transport)?;

    Ok(())
}
