//! Patching: giving each segment an address (output, port, channel), plus a
//! direction and an offset (names.html "Patching"). The patcher walks one
//! fixture's tree with a cursor; starts, flows and overrides move it by
//! Keep / Set / Add.
//!
//! Saved with the parts as map2d format 6 ([`crate::map2d6`]). Nothing
//! outside this crate calls it yet: the engine still runs `PatchDoc`.

mod address;
mod amount;
mod cursor;
mod direction;
mod flow;
mod output_name;
mod output_ports;
mod override_entry;
mod patch_target;
mod patcher;
mod patching;
mod port_key;
mod problem;
mod segment_patch;
mod span;
mod start;
mod step_source;
mod walked;

pub use address::Address;
pub use amount::{Amount, Unit};
pub use cursor::Cursor;
pub use direction::Direction;
pub use flow::Flow;
pub use output_name::OutputName;
pub use output_ports::{OutputPorts, Port};
pub use override_entry::Override;
pub use patch_target::PatchTarget;
pub use patcher::patch_mapping;
pub use patching::Patching;
pub use port_key::PortKey;
pub use problem::Problem;
pub use segment_patch::SegmentPatch;
pub use span::{PortSpans, Span};
pub use start::{ChannelOp, OutputOp, PortOp, Start};
pub use step_source::StepSource;
pub use walked::Walked;
