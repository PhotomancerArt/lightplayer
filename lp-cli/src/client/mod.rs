// Re-export the host/native client shape the CLI uses.
#[allow(
    unused_imports,
    reason = "CLI modules import lpa-client host API through this compatibility surface"
)]
pub use lpa_client::{
    ClientTransport, HostSpecifier, TokioLpClient as LpClient, WebSocketClientTransport,
    create_local_transport_pair, transport, transport_emu_serial, transport_serial,
};

// CLI-specific modules
pub mod client_connect;
pub mod host_process;
pub mod host_serial_esp32;
pub mod serial_port;

// Re-export CLI-specific types
pub use client_connect::client_connect;
