//! App-side links to LightPlayer runtimes and devices.

pub mod provider;
pub mod providers;
pub mod registry;

#[cfg(any(feature = "host-process", feature = "host-serial-esp32"))]
pub use provider::connection::{LinkClientTransport, LinkServerConnection};
pub use provider::connection::{LinkConnection, LinkConnectionKind};
pub use provider::diagnostic::{LinkDiagnostic, LinkDiagnosticSeverity};
pub use provider::endpoint::LinkEndpoint;
pub use provider::endpoint::LinkEndpointId;
pub use provider::endpoint::LinkEndpointStatus;
pub use provider::error::LinkError;
pub use provider::log::{LinkLogEntry, LinkLogLevel};
pub use provider::management_event::{
    LinkManagementEvent, LinkManagementEventSink, emit_management_result_events,
};
pub use provider::management_progress::LinkManagementProgress;
pub use provider::management_request::LinkManagementRequest;
pub use provider::management_result::{
    LinkEraseDeviceResult, LinkFirmwareFlashResult, LinkFirmwareManifest, LinkManagementResult,
    LinkRawFilesystemEraseResult,
};
pub use provider::operation::{LinkCapabilities, LinkOperation};
pub use provider::provider::LinkProvider;
pub use provider::session::LinkSession;
pub use provider::session::LinkSessionId;
pub use provider::session::LinkSessionStatus;
pub use providers::LinkProviderKind;
