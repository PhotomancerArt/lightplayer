//! The ports each fixture on the Graph is patched against (lean Q75): the
//! named outputs its product reaches, each output's ports cut by the
//! engine's own walk over its authored ports
//! ([`lpc_engine::node::output_port_slices`], the one the wires are cut by
//! and the engine's port tables are made from), so an address the Graph
//! shows and the wire it lands on on the board can't disagree.
//!
//! Capacity is the slice's samples, which the patcher reads as channels (a
//! sample is one channel of an RGB lamp, all a board drives today), as the
//! engine's `parts_patch.rs` does. An output with a count-less port before
//! its last drives no wires, and is offered with no ports (the engine's
//! `remainder_offender`).

use std::collections::BTreeMap;

use lab_mapping_files::ProductProject;
use lab_mapping_model::{NodeId, Project};
use lpc_engine::node::output_port_slices;
use lpc_mapping::patching::{OutputName, OutputPorts, Port, PortKey};

/// Every fixture's port tables, by fixture.
pub(crate) fn port_tables(
    project: &Project,
    files: &ProductProject,
) -> BTreeMap<NodeId, Vec<OutputPorts>> {
    project
        .fixtures()
        .iter()
        .map(|n| {
            let mut tables: Vec<OutputPorts> = Vec::new();
            for output in files.reachable_outputs(project, &n.id) {
                let Some(table) = files.nodes.get(&output).and_then(|f| output_ports(&f.text))
                else {
                    continue;
                };
                if !tables.iter().any(|t| t.output == table.output) {
                    tables.push(table);
                }
            }
            (n.id.clone(), tables)
        })
        .collect()
}

/// One output's table from its node file's text (as authored: the slot
/// reader streams, so the file's own key order, `kind` first, is what it
/// reads), when it is a named output.
fn output_ports(text: &str) -> Option<OutputPorts> {
    let def = lpc_model::NodeDef::from_json_str(text).ok()?;
    let def = def.as_output()?;
    let name = def.output_name()?;
    let slices: Vec<_> = output_port_slices(def).map(|(_, slice)| slice).collect();
    let last = slices.len().saturating_sub(1);
    let offender = slices[..last]
        .iter()
        .any(|slice| slice.capacity_samples.is_none());
    Some(OutputPorts {
        output: OutputName::new(name.as_str()),
        ports: if offender {
            Vec::new()
        } else {
            slices
                .iter()
                .map(|slice| Port {
                    key: PortKey(slice.key),
                    channels: slice.capacity_samples,
                })
                .collect()
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An output's table is its authored ports in key order, each a count
    /// of lamps times three channels, and a last port with no count takes
    /// the rest.
    #[test]
    fn a_named_outputs_ports_are_its_authored_slices() {
        let text = r#"{
  "kind": "Output",
  "name": "main",
  "ports": {
    "1": { "endpoint": "ws281x:local:D1" },
    "0": { "endpoint": "ws281x:local:D0", "count": 64 }
  }
}"#;
        let table = output_ports(text).expect("a named output");
        assert_eq!(table.output, OutputName::new("main"));
        assert_eq!(
            table.ports,
            vec![
                Port {
                    key: PortKey(0),
                    channels: Some(192)
                },
                Port {
                    key: PortKey(1),
                    channels: None
                },
            ]
        );
    }

    /// A repo output file, `bindings` and all, reads as its table: the
    /// file is read as authored (a reordered copy, `bindings` before
    /// `kind`, is refused by the slot reader).
    #[test]
    fn an_output_with_bindings_reads_as_its_table() {
        let text = r#"{
  "kind": "Output",
  "name": "1",
  "ports": { "0": { "endpoint": "ws281x:local:D10" } },
  "bindings": { "input": { "source": "bus:control.out" } }
}"#;
        let table = output_ports(text).expect("a named output");
        assert_eq!(table.output, OutputName::new("1"));
        assert_eq!(
            table.ports,
            vec![Port {
                key: PortKey(0),
                channels: None
            }]
        );
    }

    /// An output with no name can't be named by a start, so it has no
    /// table.
    #[test]
    fn an_unnamed_output_has_no_table() {
        let text = r#"{
  "kind": "Output",
  "ports": { "0": { "endpoint": "ws281x:local:D0", "count": 8 } }
}"#;
        assert_eq!(output_ports(text), None);
    }
}
