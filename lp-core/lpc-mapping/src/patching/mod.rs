//! Patching: giving each segment an address (output, port, channel), plus a
//! direction and an offset (names.html "Patching"). The patcher walks one
//! fixture's tree with a cursor; starts, flows and overrides move it by
//! Keep / Set / Add.
//!
//! In memory only: no file format reads or writes these yet, and nothing
//! outside this crate calls them (the engine still runs `PatchDoc`).

mod address;
mod amount;
mod cursor;
mod direction;
mod flow;
mod output_name;
mod output_ports;
mod override_entry;
mod patch_target;
mod port_key;
mod start;
mod step_source;

pub use address::Address;
pub use amount::{Amount, Unit};
pub use cursor::Cursor;
pub use direction::Direction;
pub use flow::Flow;
pub use output_name::OutputName;
pub use output_ports::{OutputPorts, Port};
pub use override_entry::Override;
pub use patch_target::PatchTarget;
pub use port_key::PortKey;
pub use start::{ChannelOp, OutputOp, PortOp, Start};
pub use step_source::StepSource;
