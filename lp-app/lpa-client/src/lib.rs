//! LightPlayer client library.
//!
//! Provides the typed client-side protocol for communicating with `LpServer`.
//! The core `LpClient<Io>` is runtime-neutral; host transports and the
//! cloneable Tokio wrapper live behind host-oriented feature flags.

pub mod client;
pub mod client_error;
pub mod client_event;
pub mod client_io;
pub mod file_sync_ops;
#[cfg(feature = "host")]
pub mod local;
pub mod project_deploy;
pub mod project_read_stream;
pub mod protocol_session;
pub mod pull_loop;
#[cfg(feature = "host")]
pub mod specifier;
pub mod stream;
#[cfg(feature = "host")]
pub mod tokio_client;
#[cfg(feature = "host")]
pub mod transport;
#[cfg(feature = "emu")]
pub mod transport_emu_serial;
#[cfg(feature = "serial")]
pub mod transport_serial;
#[cfg(feature = "ws")]
pub mod transport_ws;

// Re-export main types
pub use client::{ClientOutcome, LpClient};
pub use client_error::{ClientError, ClientResult};
pub use client_event::ClientEvent;
pub use client_io::ClientIo;
#[cfg(feature = "host")]
pub use local::{
    AsyncLocalClientTransport, AsyncLocalServerTransport, create_local_transport_pair,
};
pub use project_deploy::ProjectDeployFile;
pub use pull_loop::{
    BackoffPolicy, CancelSignal, NeverCancel, ProgressDeadline, PullOutcome, run_project_read,
};
#[cfg(feature = "host")]
pub use specifier::HostSpecifier;
#[cfg(feature = "serial")]
pub use stream::SerialPortByteStream;
pub use stream::{ByteStreamError, DeviceByteStream};
#[cfg(feature = "host")]
pub use tokio_client::{SharedClientTransport, TokioClientIo, TokioLpClient};
#[cfg(feature = "host")]
pub use transport::ClientTransport;
#[cfg(feature = "ws")]
pub use transport_ws::WebSocketClientTransport;
