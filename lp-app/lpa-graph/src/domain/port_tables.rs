//! The ports each fixture is patched against: the named outputs it can
//! reach, each with its ports' capacities in channels
//! (`lpc_mapping::patching::OutputPorts`).
//!
//! **Studio hands them over** (lean Q75): core builds each fixture's tables
//! with the engine's own walk over every output's authored ports
//! (`lpc_engine::node::output_port_slices`, the one the wires are cut by),
//! from the outputs that read the fixture's bus channel and have a name, so
//! what the Graph shows and what the board does can't disagree. The tables
//! are a fact the project carries beside what it holds, as the placement
//! is: handing new ones over re-patches without an edit.
//!
//! **Without them** (the lab's page and its tests, which have no engine)
//! the project works them out from its own outputs the same way: every
//! named output, ports in key order, a port's count in lamps of three
//! channels, a port with no count taking the rest when it is the last one
//! (an output with a count-less port before its last drives no wires, so
//! it has no ports at all).

use std::collections::BTreeMap;
use std::sync::Arc;

use lpc_mapping::patching::{OutputName, OutputPorts, Port, PortKey};

use crate::domain::node::NodeId;
use crate::domain::project::Project;

/// Channels a lamp of a port's count stands for: ports count RGB lamps
/// (the engine's `PORT_SAMPLES_PER_LAMP`).
pub const PORT_CHANNELS_PER_LAMP: u32 = 3;

/// The tables a host handed over, by fixture. Never compared: like the
/// placement, it is not part of what the project holds.
#[derive(Debug, Clone, Default)]
pub(crate) struct CarriedPortTables(pub(crate) Option<Arc<BTreeMap<NodeId, Vec<OutputPorts>>>>);

impl PartialEq for CarriedPortTables {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl Project {
    /// The ports a fixture is patched against: the host's tables when it
    /// handed some over, else the project's own outputs' (module docs).
    pub fn port_tables(&self, fixture: &NodeId) -> Vec<OutputPorts> {
        match &self.port_tables.0 {
            Some(tables) => tables.get(fixture).cloned().unwrap_or_default(),
            None => self.own_port_tables(),
        }
    }

    /// Take the host's port tables, by fixture. Returns whether they
    /// changed; when they did, the patch is worked out again (the revision
    /// stays: what the project holds is the same).
    pub fn set_port_tables(&mut self, tables: BTreeMap<NodeId, Vec<OutputPorts>>) -> bool {
        if self.port_tables.0.as_deref() == Some(&tables) {
            return false;
        }
        self.port_tables.0 = Some(Arc::new(tables));
        self.moved();
        true
    }

    /// The output node a patcher's output name means: the first output in
    /// attach order with that name.
    pub fn output_named(&self, name: &str) -> Option<NodeId> {
        self.attach_order()
            .into_iter()
            .find(|n| n.output().is_some_and(|o| o.label.as_deref() == Some(name)))
            .map(|n| n.id.clone())
    }

    /// Every named output's ports, as the patcher reads them.
    fn own_port_tables(&self) -> Vec<OutputPorts> {
        let mut out: Vec<OutputPorts> = Vec::new();
        for n in self.attach_order() {
            let Some(o) = n.output() else { continue };
            let Some(name) = o.label.as_deref() else {
                continue;
            };
            if out.iter().any(|t| t.output.as_str() == name) {
                continue;
            }
            let mut ports: Vec<(u32, Option<u32>)> = o
                .ports
                .iter()
                .filter_map(|p| Some((p.key.parse().ok()?, p.pixels)))
                .collect();
            ports.sort_by_key(|(k, _)| *k);
            let last = ports.len().saturating_sub(1);
            let offender = ports.iter().take(last).any(|(_, n)| n.is_none());
            out.push(OutputPorts {
                output: OutputName::new(name),
                ports: if offender {
                    Vec::new()
                } else {
                    ports
                        .into_iter()
                        .map(|(key, lamps)| Port {
                            key: PortKey(key),
                            channels: lamps.map(|n| n * PORT_CHANNELS_PER_LAMP),
                        })
                        .collect()
                },
            });
        }
        out
    }
}
