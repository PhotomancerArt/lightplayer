//! Serve command handler
//!
//! Orchestrates the serve command execution.

use anyhow::Result;
use std::path::PathBuf;

use super::args::ServeArgs;
use super::init::create_filesystem;
use crate::client::relay_session::cloud_session_from_env;
use crate::server::create_server::create_server_on;
use crate::server::relay_host::{
    host_board_id, host_board_label, install_account_key, relay_accounts, start_relay_host,
};
use crate::server::transport_ws::WebSocketServerTransport;
use crate::server::{run_server_loop_async, run_server_loop_with};

/// Handle the serve command
///
/// Initializes server, creates filesystem, starts LpServer, and runs the main loop.
pub fn handle_serve(args: ServeArgs) -> Result<()> {
    // Determine server directory (default to current directory)
    let server_dir = args.dir.unwrap_or_else(|| PathBuf::from("../../../.."));

    let runtime = tokio::runtime::Runtime::new()
        .map_err(|e| anyhow::anyhow!("Failed to create runtime: {e}"))?;

    let base_fs = create_filesystem(Some(&server_dir), args.memory)?;
    // The relay host board's account: installed before the server reads
    // its store. The session is a credential: env only, never printed.
    if let (Some(origin), Some(session)) = (&args.relay, cloud_session_from_env()) {
        let name = runtime.block_on(install_account_key(&*base_fs, origin, &session))?;
        println!("installed {name}'s account key");
    }
    let accounts = relay_accounts(&*base_fs);
    let mut server = create_server_on(base_fs, Some(&server_dir), args.memory, Some(args.init))?;

    // Create websocket server transport on port 2812
    let transport = WebSocketServerTransport::new(2812)
        .map_err(|e| anyhow::anyhow!("Failed to start websocket server: {e}"))?;

    println!("Server started on ws://localhost:2812/");
    let Some(origin) = args.relay else {
        println!("Press Ctrl+C to stop");
        // Run server loop (blocks until interrupted)
        return runtime.block_on(run_server_loop_async(server, transport));
    };

    // Login challenges (a relay visitor's password) need real randomness.
    server.set_entropy_source(Some(lpa_client::transport_lan::os_entropy));
    let seed = if args.memory {
        "memory".to_string()
    } else {
        std::fs::canonicalize(&server_dir)
            .unwrap_or(server_dir.clone())
            .display()
            .to_string()
    };
    let board = host_board_id(&seed);
    if accounts.is_empty() {
        println!(
            "relay: this board holds no account key, so it will not dial {origin}; set LP_CLOUD_SESSION to install yours"
        );
    }
    println!("relay: board {board} on {origin} (connect with relay:{board}@{origin})");
    println!("Press Ctrl+C to stop");
    runtime.block_on(async {
        let mut transport =
            start_relay_host(transport, &origin, board, host_board_label(), accounts)?;
        // The project's facts before the leg's first registration.
        transport.report_project(&server);
        run_server_loop_with(server, transport, |server, transport| {
            transport.after_tick(server);
        })
        .await
    })
}
