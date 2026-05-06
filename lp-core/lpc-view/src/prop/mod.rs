//! Legacy property access views over cached portable values.
//!
//! New client code should prefer [`crate::SlotMirrorView`], which preserves
//! slot shape, container versions, and mutation state.

pub mod prop_access_view;

pub use prop_access_view::{PropAccessView, PropsMapView};
