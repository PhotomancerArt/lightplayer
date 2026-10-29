//! The port tables a fixture patches against: the outputs that read its bus
//! channel, each by its name, with its ports' slices of the output's buffer
//! ([`crate::node::output_port_slices`], the same walk the wires are cut by).
//!
//! Engine-native on purpose: the output side is never gated out while the
//! mapping crate can be (`node-fixture` gates `lpc-mapping`), so these name
//! no `lpc-mapping` type. A format-6 fixture converts them into the
//! patcher's `OutputPorts` itself.

use alloc::string::String;
use alloc::vec::Vec;

use lpc_model::{ChannelName, Revision};

use crate::node::output_port_slice::OutputPortSlice;

/// One output a fixture may name, and its ports.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputPortTable {
    /// The output's name, as patch routing matches it.
    pub name: String,
    /// Its ports in key order, gaps in the keys kept. Empty when the output
    /// drives no wires at all (a count-less port that isn't its last).
    pub ports: Vec<OutputPortSlice>,
}

/// What a fixture can reach: the bus channel its product is published on,
/// and the named outputs that read it there.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReachableOutputs {
    /// The channel the producer writes, for a problem that has to name it.
    /// `None` when the product is bound to no bus channel.
    pub channel: Option<ChannelName>,
    /// The named outputs gathering the product from that channel, by name.
    pub outputs: Vec<OutputPortTable>,
}

/// What [`ReachableOutputs`] were read under: it moves whenever an output's
/// name, a port, a count, or the set of outputs on a channel can have
/// changed. Only equality across two observations means anything.
///
/// The structure epoch moves on every def edit (a slot edit included, live)
/// and on every node attached or removed (frame-plan ADR,
/// `docs/adr/2026-10-24-frame-plan-once-per-edit.md`); the output-identity
/// revision moves when an output registers or changes its name, which a
/// bound name can do without a structural edit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PortTablesKey {
    pub structure_epoch: u64,
    pub output_identities: Revision,
}
