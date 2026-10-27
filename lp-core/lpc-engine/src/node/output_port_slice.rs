//! Where each port of an output sits in the output's buffer: ONE walk over
//! the authored ports, shared by the wires that drive them
//! (`engine_services::planned_wires`) and the port tables a fixture patches
//! against ([`crate::node::OutputPortTable`]). One function is what keeps an
//! address the patcher gives and the wire it lands on from ever disagreeing.

use lpc_model::nodes::output::{OutputDef, OutputPortDef};

/// Samples a lamp's port count stands for: ports count RGB lamps today, and
/// a lamp is three samples (the output's `SAMPLES_PER_LAMP`).
pub const PORT_SAMPLES_PER_LAMP: u32 = 3;

/// One authored port's slice of its output's buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OutputPortSlice {
    /// The port's authored key.
    pub key: u32,
    /// The first sample of the output's buffer this port carries.
    pub start_samples: u32,
    /// How many samples it carries: `None` takes the rest (only the last
    /// port may), `Some(0)` none at all (a port authored with no lamps).
    pub capacity_samples: Option<u32>,
}

/// Walk an output's ports in ascending key order, each with its slice of
/// the output's buffer: slices start where the previous one ended, a port's
/// `count` is in lamps of [`PORT_SAMPLES_PER_LAMP`] samples, and a port with
/// no count takes the rest.
///
/// Every port is yielded, a `Some(0)` one included (the wires drop it, the
/// port tables keep it with no room). Whether a count-less port is the last
/// one is the caller's check (`remainder_offender`): this walk assumes it.
///
/// Allocation-free: the per-tick wire diff drives it directly.
pub fn output_port_slices(
    config: &OutputDef,
) -> impl Iterator<Item = (&OutputPortDef, OutputPortSlice)> + '_ {
    let mut start = 0u32;
    config.ports.entries.iter().map(move |(key, port)| {
        let capacity = port
            .count()
            .map(|count| count.saturating_mul(PORT_SAMPLES_PER_LAMP));
        let slice = OutputPortSlice {
            key: *key,
            start_samples: start,
            capacity_samples: capacity,
        };
        if let Some(len) = capacity {
            start = start.saturating_add(len);
        }
        (port, slice)
    })
}
