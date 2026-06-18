//! App-side links to LightPlayer runtimes and devices.

pub mod link_connection;
pub mod link_diagnostic;
pub mod link_endpoint;
pub mod link_endpoint_id;
pub mod link_endpoint_status;
pub mod link_error;
pub mod link_log_entry;
pub mod link_management;
pub mod link_provider;
pub mod link_provider_id;
pub mod link_session;
pub mod link_session_id;
pub mod providers;

#[cfg(any(feature = "host-process", feature = "host-serial-esp32"))]
pub use link_connection::{LinkClientTransport, LinkServerConnection};
pub use link_connection::{LinkConnection, LinkConnectionKind};
pub use link_diagnostic::{LinkDiagnostic, LinkDiagnosticSeverity};
pub use link_endpoint::LinkEndpoint;
pub use link_endpoint_id::LinkEndpointId;
pub use link_endpoint_status::LinkEndpointStatus;
pub use link_error::LinkError;
pub use link_log_entry::{LinkLogEntry, LinkLogLevel};
pub use link_management::{LinkManagement, LinkManagementOperation};
pub use link_provider::LinkProvider;
pub use link_provider_id::LinkProviderId;
pub use link_session::LinkSession;
pub use link_session_id::LinkSessionId;
