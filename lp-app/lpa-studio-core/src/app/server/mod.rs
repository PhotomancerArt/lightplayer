#[cfg(all(feature = "browser-serial-esp32", target_arch = "wasm32"))]
mod browser_serial_client_io;
mod browser_serial_readiness;
#[cfg(all(feature = "browser-worker", target_arch = "wasm32"))]
mod browser_worker_client_io;
pub mod browser_worker_log;
pub mod device_log_line;
/// Test-edge host `ClientIo` over fake-device link connections. `cfg(test)`
/// keeps lpa-studio-core sans-IO in product builds; the e2e matrix drives
/// the real link path through it (see the module docs).
#[cfg(all(test, not(target_arch = "wasm32")))]
mod fake_link_client_io;
mod pending_server_messages;
pub mod server_controller;
pub mod server_op;
pub mod server_snapshot;
pub mod server_state;
pub mod studio_server_client;

pub use server_controller::ServerController;
pub use server_op::ServerOp;
pub use server_snapshot::ServerSnapshot;
pub use server_state::{ServerFailureKind, ServerState};
pub use studio_server_client::{
    LoadedDemoProject, LoadedProjectCatalog, LoadedRunningProject, StudioFsRead,
    StudioOverlayCommit, StudioOverlayMutation, StudioOverlayRead, StudioProjectRead,
    StudioProjectReadOutcome, StudioServerClient,
};
