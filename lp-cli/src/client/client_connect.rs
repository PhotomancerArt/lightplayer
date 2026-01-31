//! Client transport connection factory
//!
//! Provides `client_connect()` function that creates appropriate `ClientTransport`
//! based on a `HostSpecifier`.

use crate::client::transport::ClientTransport;
use anyhow::{Result, bail};

use crate::client::local_server::LocalServerTransport;
use crate::client::specifier::HostSpecifier;
use crate::client::transport_ws::WebSocketClientTransport;

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
/// use lp_cli::client::{client_connect, specifier::HostSpecifier};
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
}
