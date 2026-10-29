//! An output's ports, as the patcher sees them (the engine supplies them).

use alloc::vec::Vec;

use crate::patching::output_name::OutputName;
use crate::patching::port_key::PortKey;

/// One output and its ports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputPorts {
    pub output: OutputName,
    /// In any order; the patcher takes them in key order.
    pub ports: Vec<Port>,
}

/// One port of an output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Port {
    pub key: PortKey,
    /// Capacity, in channels. `None` takes the rest on the last port and
    /// has no room on any other (today's rule).
    pub channels: Option<u32>,
}

impl OutputPorts {
    /// An output with ports keyed 0, 1, … of these capacities in lamps of
    /// `per_lamp` channels (`None`: takes the rest).
    #[must_use]
    pub fn of_lamps(output: &str, lamps: &[Option<u32>], per_lamp: u32) -> Self {
        Self {
            output: OutputName::new(output),
            ports: lamps
                .iter()
                .enumerate()
                .map(|(k, lamps)| Port {
                    key: PortKey(k as u32),
                    channels: lamps.map(|n| n * per_lamp),
                })
                .collect(),
        }
    }
}
