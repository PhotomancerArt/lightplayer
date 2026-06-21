//! App-side links to LightPlayer runtimes and devices.

pub mod link_connection;
pub mod link_diagnostic;
pub mod link_endpoint;
pub mod link_error;
pub mod link_log_entry;
pub mod link_operation;
pub mod link_provider;
pub mod link_session;
pub mod providers;

#[cfg(any(feature = "host-process", feature = "host-serial-esp32"))]
pub use link_connection::{LinkClientTransport, LinkServerConnection};
pub use link_connection::{LinkConnection, LinkConnectionKind};
pub use link_diagnostic::{LinkDiagnostic, LinkDiagnosticSeverity};
pub use link_endpoint::LinkEndpoint;
pub use link_endpoint::LinkEndpointId;
pub use link_endpoint::LinkEndpointStatus;
pub use link_error::LinkError;
pub use link_log_entry::{LinkLogEntry, LinkLogLevel};
pub use link_operation::{LinkCapabilities, LinkOperation};
pub use link_provider::LinkProvider;
pub use link_provider::LinkProviderId;
pub use link_session::LinkSession;
pub use link_session::LinkSessionId;
pub use link_session::LinkSessionStatus;
