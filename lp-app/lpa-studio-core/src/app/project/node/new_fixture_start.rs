//! Where a fixture created in Studio starts: on the first named output its
//! bus channel reaches.
//!
//! The fixture starter's mapping
//! ([`lpc_model::nodes::starter::STARTER_FIXTURE_MAP2D`], map2d format 6)
//! starts on output `"1"`, the name a starter project's one
//! output carries. A project whose output is called something else would
//! leave the new fixture dark, so the create rewrites that start to the
//! first named output the fixture's control product reaches: the outputs
//! consuming [`lpc_model::PRIMARY_CONTROL_CHANNEL`] (the channel a fixture's
//! `output` publishes to by default) in the scope the fixture is created
//! in, in tree order. Read off the lens's binding graph, the same structure
//! the Graph and the panels read, so what counts as "reached" is what the
//! engine resolved rather than a second guess at the binding rules.
//!
//! When no named output is reached (no graph held yet, no output on the
//! channel, or only unnamed ones) the starter is left as it is.

use std::collections::BTreeSet;

use lpc_mapping::map2d6::{self, Map2d6Error};
use lpc_mapping::patching::{OutputName as StartOutputName, OutputOp};
use lpc_model::{NodeId, OutputName, PRIMARY_CONTROL_CHANNEL};
use lpc_wire::{WireBindingGraph, WireScopeRef};

use super::new_output_name::authored_output_name;
use super::node_controller::NodeController;

/// The first named output (tree order) consuming the primary control
/// channel in the scope `scope_owner`'s module introduces, if any.
pub(in crate::app::project) fn first_reached_output_name(
    roots: &[NodeController],
    graph: &WireBindingGraph,
    scope_owner: NodeId,
) -> Option<OutputName> {
    let scope = WireScopeRef::Module { owner: scope_owner };
    let channel = graph.channels.iter().find(|channel| {
        channel.name == PRIMARY_CONTROL_CHANNEL && channel.scope.as_ref() == Some(&scope)
    })?;
    let consumers: BTreeSet<NodeId> = channel
        .consumers
        .iter()
        .filter_map(|index| graph.bindings.get(*index as usize))
        .map(|binding| binding.node)
        .collect();
    roots
        .iter()
        .find_map(|root| first_named_consumer(root, &consumers))
}

/// `map2d` (a format-6 document) with its start's output set to `output`,
/// in the canonical spelling [`map2d6::write`] gives it.
pub(in crate::app::project) fn map2d_starting_on(
    map2d: &str,
    output: &OutputName,
) -> Result<String, Map2d6Error> {
    let mut mapping = map2d6::read(map2d)?;
    mapping.start.output = OutputOp::Set(StartOutputName::new(output.as_str()));
    map2d6::write(&mapping)
}

fn first_named_consumer(node: &NodeController, consumers: &BTreeSet<NodeId>) -> Option<OutputName> {
    if consumers.contains(&node.target().node_id)
        && let Some(name) = authored_output_name(node)
    {
        return Some(name);
    }
    node.children()
        .iter()
        .find_map(|child| first_named_consumer(child, consumers))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_model::nodes::starter::STARTER_FIXTURE_MAP2D as STARTER;

    #[test]
    fn the_starter_mapping_moves_to_the_named_output_and_keeps_the_rest() {
        let moved = map2d_starting_on(STARTER, &OutputName::from_static("front"))
            .expect("the starter mapping reads");
        assert_eq!(moved, STARTER.replace(r#""set": "1""#, r#""set": "front""#));
        assert_eq!(
            map2d_starting_on(STARTER, &OutputName::from_static("1")).expect("reads"),
            STARTER,
            "on output 1 the starter is its own canonical spelling",
        );
    }
}
