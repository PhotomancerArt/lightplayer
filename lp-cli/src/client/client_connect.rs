//! Client transport connection factory
//!
//! Provides `client_connect()` function that creates appropriate `ClientTransport`
//! based on a `HostSpecifier`.

use anyhow::{Context, Result, bail};
#[cfg(feature = "serial")]
use lp_client::transport_serial::create_emulator_serial_transport_pair;
use lp_client::{ClientTransport, HostSpecifier, WebSocketClientTransport};
#[cfg(feature = "serial")]
use lp_riscv_elf::load_elf;
#[cfg(feature = "serial")]
use lp_riscv_emu::{
    LogLevel, Riscv32Emulator, TimeMode,
    test_util::{BinaryBuildConfig, ensure_binary_built},
};
#[cfg(feature = "serial")]
use lp_riscv_inst::Gpr;
#[cfg(feature = "serial")]
use std::sync::{Arc, Mutex};

use crate::client::local_server::LocalServerTransport;

/// Connect to a server using the specified host specifier
///
/// Creates and returns an appropriate `ClientTransport` based on the `HostSpecifier`.
/// For `Local`, creates an in-memory server on a separate thread.
///
/// # Arguments
///
/// * `spec` - Host specifier indicating transport type and connection details
///
/// # Returns
///
/// * `Ok(Box<dyn ClientTransport + Send>)` if connection succeeded
/// * `Err` if connection failed or transport type not supported
///
/// # Examples
///
/// ```
/// use lp_cli::client::client_connect;
/// use lp_client::HostSpecifier;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// // Connect to local in-memory server
/// let mut transport = client_connect(HostSpecifier::Local)?;
/// // Note: In real usage, you would use the transport and then close it.
/// // For doctest purposes, we just demonstrate creation.
///
/// // Connect to websocket server (will fail without a running server, but shows usage)
/// let spec = HostSpecifier::parse("ws://localhost:2812/")?;
/// // Note: This would connect to a websocket server if one was running
/// # Ok(())
/// # }
/// ```
pub fn client_connect(spec: HostSpecifier) -> Result<Box<dyn ClientTransport>> {
    match spec {
        HostSpecifier::Local => {
            // Create local server transport (now implements ClientTransport directly)
            let local_server = LocalServerTransport::new()?;
            Ok(Box::new(local_server))
        }
        HostSpecifier::WebSocket { url } => {
            // WebSocketClientTransport::new is async, but client_connect is sync
            // We need to use tokio runtime to connect
            let rt = tokio::runtime::Runtime::new()
                .map_err(|e| anyhow::anyhow!("Failed to create tokio runtime: {e}"))?;
            let transport = rt
                .block_on(WebSocketClientTransport::new(&url))
                .map_err(|e| anyhow::anyhow!("Failed to connect to {url}: {e}"))?;
            Ok(Box::new(transport))
        }
        HostSpecifier::Serial { .. } => {
            bail!("Serial transport not yet implemented");
        }
        #[cfg(feature = "serial")]
        HostSpecifier::Emulator => {
            // Build fw-emu binary
            let fw_emu_path = ensure_binary_built(
                BinaryBuildConfig::new("fw-emu")
                    .with_target("riscv32imac-unknown-none-elf")
                    .with_profile("release"),
            )
            .map_err(|e| anyhow::anyhow!("Failed to build fw-emu: {e}"))?;

            // Load ELF
            let elf_data = std::fs::read(&fw_emu_path).context("Failed to read fw-emu ELF")?;
            let load_info =
                load_elf(&elf_data).map_err(|e| anyhow::anyhow!("Failed to load ELF: {e}"))?;

            // Create emulator with real time mode
            // Use a higher instruction limit for complex scenes (100M instructions)
            let ram_size = load_info.ram.len();
            let mut emulator = Riscv32Emulator::new(load_info.code, load_info.ram)
                .with_log_level(LogLevel::None)
                .with_time_mode(TimeMode::RealTime);

            // Set up stack pointer
            let sp_value = 0x80000000u32.wrapping_add((ram_size as u32).wrapping_sub(16));
            emulator.set_register(Gpr::Sp, sp_value as i32);

            // Set PC to entry point
            emulator.set_pc(load_info.entry_point);

            // Create shared emulator reference
            let emulator_arc = Arc::new(Mutex::new(emulator));

            // Create async serial transport
            let transport = create_emulator_serial_transport_pair(emulator_arc)
                .map_err(|e| anyhow::anyhow!("Failed to create emulator serial transport: {e}"))?;

            Ok(Box::new(transport))
        }
        #[cfg(not(feature = "serial"))]
        HostSpecifier::Emulator => {
            bail!("Emulator transport requires 'serial' feature to be enabled");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_client_connect_local() {
        let spec = HostSpecifier::Local;
        let result = client_connect(spec);
        assert!(result.is_ok());
        let mut transport = result.unwrap();
        // Verify we can call methods on it
        // Note: receive() is async and will wait, so we'll just test close
        let _ = transport.close().await; // Should close successfully
    }

    #[test]
    fn test_client_connect_websocket() {
        // This test would require a running websocket server
        // For now, just verify it parses correctly and attempts connection
        let spec = HostSpecifier::parse("ws://localhost:2812/").unwrap();
        let result = client_connect(spec);
        // Will likely fail to connect without a server, but should parse correctly
        // We can't easily test connection without a server, so we just verify it doesn't panic
        let _ = result;
    }

    #[test]
    fn test_client_connect_serial() {
        let spec = HostSpecifier::parse("serial:auto").unwrap();
        let result = client_connect(spec);
        assert!(result.is_err());
        if let Err(e) = result {
            assert!(format!("{e}").contains("not yet implemented"));
        }
    }

    #[test]
    #[cfg(feature = "serial")]
    fn test_client_connect_emulator() {
        let spec = HostSpecifier::parse("emu").unwrap();
        let result = client_connect(spec);
        // This will build fw-emu, so it may be slow, but should succeed
        assert!(result.is_ok());
        let mut transport = result.unwrap();
        // Close it to clean up
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let _ = transport.close().await;
        });
    }
}
