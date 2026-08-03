//! Bus channel rows — the module card's **wiring** drawer.
//!
//! Built for the sidebar bus pane, which is gone (P3). [`WiringDrawerBody`] is
//! now mounted by [`crate::app::module::ModuleFace`]'s wiring drawer, once
//! per module scope, with the rows unchanged — the split is deliberate:
//! bus-as-controls is the panel on the face, bus-as-writers/readers is the
//! drawer under it. The colocated stories moved with it, to
//! `app/module/wiring_drawer_stories.rs`.

pub(crate) mod wiring_drawer;

pub use wiring_drawer::WiringDrawerBody;
