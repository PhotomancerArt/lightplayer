//! Bus **wiring** view models.
//!
//! A derived view over the binding-graph probe snapshot
//! (docs/adr/2026-07-06-binding-graph-probe.md); it owns no state of its
//! own. `ProjectController::ui_bus_view_for_scope` performs the projection
//! so node labels and focus actions come from the same controllers the
//! project pane uses.
//!
//! The sidebar bus pane these types were built for is GONE (P3). Their
//! home is now the module card's **wiring** drawer, one view per scope,
//! hung off the module that owns it — same rows, relocated.

pub mod ui_bus_view;

pub use ui_bus_view::{
    UiBusChannelPreview, UiBusChannelView, UiBusSiteOrigin, UiBusSiteView, UiBusView,
};
