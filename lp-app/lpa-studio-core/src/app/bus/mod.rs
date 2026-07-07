//! Bus pane view models.
//!
//! The bus pane is a derived view over the binding-graph probe snapshot
//! (docs/adr/2026-07-06-binding-graph-probe.md); it owns no state of its
//! own. `ProjectController::ui_bus_view` performs the projection so node
//! labels and focus actions come from the same controllers the project
//! pane uses.

pub mod ui_bus_view;

pub use ui_bus_view::{UiBusChannelView, UiBusSiteView, UiBusView};
