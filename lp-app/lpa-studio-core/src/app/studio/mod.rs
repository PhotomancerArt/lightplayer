pub mod console_command;
/// Studio-level decoration of output-node faces: board identity (device
/// registry) and the incoming lamp extent (the upstream node's produced
/// control product) — the facts the project walk cannot see.
mod output_face_decoration;
pub mod refresh_cadence;
pub mod studio_actor;
/// End-to-end agent-flow tests: scripted fake model over the real agent →
/// iterate → overlay-edit path (host-only, like the edit e2e tests).
#[cfg(test)]
mod studio_agent_e2e_tests;
pub mod studio_command;
pub mod studio_controller;
/// End-to-end edit-flow tests against an in-process `lpa-server` (host-only
/// dev-dependency; never part of the wasm lib build).
#[cfg(test)]
mod studio_docs_e2e_tests;
#[cfg(test)]
mod studio_edit_e2e_tests;
/// End-to-end node-card face tests: controller-derived shader/fixture faces
/// with live knob/fader edits over the real overlay path (node-card P3).
#[cfg(test)]
mod studio_face_e2e_tests;
/// End-to-end tests through the REAL link path (provider → endpoint →
/// connect → readiness → pull) against the scripted byte-level fake device.
#[cfg(all(test, not(target_arch = "wasm32")))]
mod studio_link_e2e_tests;
/// End-to-end node create/remove tests (authoring P4): every picker kind
/// against a real server, playlist-entry attach, staged removal rows,
/// revert, and save-materialized deletion.
#[cfg(test)]
mod studio_node_crud_e2e_tests;
pub mod studio_snapshot;
pub mod studio_view_channel;
pub mod ui_console_view;
pub mod ui_studio_view;
pub mod unsaved_changes;
pub mod ux_update;
pub mod ux_update_sink;

pub use crate::core::error::{UiError, UiResult};
pub use crate::core::log::{
    LOG_RING_CAPACITY, LogClock, LogFilter, LogRing, STUDIO_LOG_SINK, StudioLogSink, UiLogDraft,
    UiLogEntry, UiLogLevel, UiLogOrigin, UiLogSource,
};
pub use crate::core::notice::UiNotices;
pub use crate::core::notice::{UiNotice, UiNoticeLevel};
pub use console_command::ConsoleCommand;
pub use refresh_cadence::{
    DEVICE_CARD_FEED_INTERVAL, DEVICE_HEARTBEAT_INTERVAL, DEVICE_REFRESH_INTERVAL,
    FRAME_STALE_AFTER_SECS, PASSIVE_PREEMPTIONS_BEFORE_PROMOTION, PASSIVE_REFRESH_BACKOFF_BASE,
    PASSIVE_REFRESH_BACKOFF_MAX, RefreshCadence, SIMULATOR_REFRESH_INTERVAL,
    VERDICT_CHASE_INTERVAL, VERDICT_CHASE_TICKS,
};
pub use studio_actor::{StudioActor, StudioActorOptions, StudioHandle};
pub use studio_command::StudioCommand;
pub use studio_controller::StudioController;
pub use studio_snapshot::StudioSnapshot;
pub use studio_view_channel::{
    StudioViewReceiver, StudioViewSender, ViewPublisher, studio_view_channel,
};
pub use ui_console_view::UiConsoleView;
pub use ui_studio_view::{
    UiChromeSession, UiChromeSessionStatus, UiChromeSessionTarget, UiLensRuntime, UiStudioView,
};
pub use unsaved_changes::has_unsaved_work;
pub use ux_update::{UxActivityTarget, UxUpdate};
pub use ux_update_sink::UxUpdateSink;
