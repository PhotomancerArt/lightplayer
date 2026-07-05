pub mod refresh_cadence;
pub mod studio_actor;
pub mod studio_command;
pub mod studio_controller;
/// End-to-end edit-flow tests against an in-process `lpa-server` (host-only
/// dev-dependency; never part of the wasm lib build).
#[cfg(test)]
mod studio_edit_e2e_tests;
pub mod studio_snapshot;
pub mod studio_view_channel;
pub mod ui_studio_view;
pub mod ux_update;
pub mod ux_update_sink;

pub use crate::core::error::{UiError, UiResult};
pub use crate::core::log::{LOG_RING_CAPACITY, LogRing, UiLogEntry, UiLogLevel};
pub use crate::core::notice::UiNotices;
pub use crate::core::notice::{UiNotice, UiNoticeLevel};
pub use refresh_cadence::{DEVICE_REFRESH_INTERVAL, RefreshCadence, SIMULATOR_REFRESH_INTERVAL};
pub use studio_actor::{StudioActor, StudioHandle};
pub use studio_command::StudioCommand;
pub use studio_controller::StudioController;
pub use studio_snapshot::StudioSnapshot;
pub use studio_view_channel::{
    StudioViewReceiver, StudioViewSender, ViewPublisher, studio_view_channel,
};
pub use ui_studio_view::UiStudioView;
pub use ux_update::{UxActivityTarget, UxUpdate};
pub use ux_update_sink::UxUpdateSink;
