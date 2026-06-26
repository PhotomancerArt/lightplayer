//! Logical control product values.

mod control_display_layout;
mod control_product;
mod control_sample_layout;

pub use control_display_layout::{ControlDisplayLayout, ControlLamp2d, ControlLayout2d};
pub use control_product::{ControlExtent, ControlProduct};
pub use control_sample_layout::{ControlSampleEncoding, ControlSampleLayout, ControlSampleSpan};
