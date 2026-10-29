//! The name a new output is born with.
//!
//! Since project format 12 a fixture's mapping names the output it starts
//! on, so an unnamed output is one no fixture can reach. An output Studio
//! creates (the add-node picker, the app agent's create) or pastes is
//! therefore named at birth: the lowest free number, `"1"`, `"2"`, …,
//! project-unique among outputs — [`lpc_model::next_output_name`], the
//! same rule the format-12 upgrade names outputs with (Q59).

use lpc_model::{NodeKind, OutputName, next_output_name};

use super::node_controller::NodeController;
use super::node_naming::node_kind_label;
use crate::app::project::project_controller::def_slot_value;

/// Every name an output in the live tree carries, in tree order.
pub(in crate::app::project) fn output_names(roots: &[NodeController]) -> Vec<OutputName> {
    let mut names = Vec::new();
    for root in roots {
        collect_output_names(root, &mut names);
    }
    names
}

/// The name a new output gets: the lowest number no output carries.
pub(in crate::app::project) fn new_output_name(roots: &[NodeController]) -> OutputName {
    let taken = output_names(roots);
    next_output_name(taken.iter())
}

/// The authored name of `node` when it is an output that has one.
pub(in crate::app::project) fn authored_output_name(node: &NodeController) -> Option<OutputName> {
    if node.kind() != node_kind_label(NodeKind::Output) {
        return None;
    }
    match def_slot_value(node, &["name", "some"])? {
        lpc_model::LpValue::String(text) => OutputName::parse(text.clone()).ok(),
        _ => None,
    }
}

fn collect_output_names(node: &NodeController, names: &mut Vec<OutputName>) {
    names.extend(authored_output_name(node));
    for child in node.children() {
        collect_output_names(child, names);
    }
}
