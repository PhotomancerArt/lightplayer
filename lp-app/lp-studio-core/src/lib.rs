//! UI-independent LightPlayer Studio domain model.

pub mod client_session;
pub mod connection_session;
pub mod in_flight_action;
pub mod link;
pub mod project;
pub mod project_session;
pub mod server;
pub mod studio_action;
pub mod studio_app;
pub mod studio_diagnostic;
pub mod studio_effect;
pub mod studio_event;
pub mod studio_heartbeat;
pub mod studio_log_entry;
pub mod studio_state;
pub mod ux;

pub use client_session::ClientSession;
pub use connection_session::ConnectionSession;
pub use in_flight_action::InFlightAction;
pub use link::device_access::{DeviceAccess, DeviceAccessStatus};
pub use link::device_capability::DeviceCapability;
pub use link::device_id::DeviceId;
pub use link::device_session::DeviceSession;
pub use link::{
    ConnectedDeviceState, DeviceHealthState, DeviceIssue, DeviceIssueKind, DeviceIssueSeverity,
    DeviceManagerState, LinkState, ProgressState, ProjectChoice, ProjectSelectionReason,
    ProjectStateResult, ProviderAvailability, ProviderCapability, ProviderCardState,
    ProviderCatalog, ProviderIntent, ProvisioningReason, RecoveryAction, RecoveryReason,
    TargetKind, TargetProbeResult,
};
pub use project_session::ProjectSession;
pub use studio_action::{StudioAction, StudioActionKind, StudioActionType};
pub use studio_app::StudioApp;
pub use studio_diagnostic::{StudioDiagnostic, StudioDiagnosticSeverity};
pub use studio_effect::StudioEffect;
pub use studio_event::StudioEvent;
pub use studio_heartbeat::StudioHeartbeat;
pub use studio_log_entry::{StudioLogEntry, StudioLogLevel};
pub use studio_state::StudioState;
pub use ux::action_descriptor::{ActionCategory, ActionDescriptor};
pub use ux::action_history_policy::{ActionHistoryPolicy, UndoScope};
pub use ux::action_id::ActionId;
pub use ux::action_meta::ActionMeta;
pub use ux::action_origin::ActionOrigin;

pub const BROWSER_WORKER_PROVIDER_ID: &str = "browser-worker";
pub const BROWSER_SERIAL_ESP32_PROVIDER_ID: &str = "browser-serial-esp32";
pub const HOST_PROCESS_PROVIDER_ID: &str = "host-process";
pub const HOST_SERIAL_ESP32_PROVIDER_ID: &str = "host-serial-esp32";
pub const STUDIO_DEMO_PROJECT_ID: &str = "studio-demo";
