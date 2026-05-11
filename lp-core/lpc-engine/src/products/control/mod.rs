//! Logical control product materialization contracts.

mod control_layout;
mod control_render_request;
mod control_render_target;

pub use control_layout::{ControlHint, ControlLayout, ControlSpan};
pub use control_render_request::{ControlRenderRequest, ControlSampleFormat};
pub use control_render_target::ControlRenderTarget;
pub use lpc_model::{ControlExtent, ControlProduct};
