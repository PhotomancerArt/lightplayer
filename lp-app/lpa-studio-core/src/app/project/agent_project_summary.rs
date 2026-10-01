//! [`ProjectController::agent_project_summary`]: the compact `project`
//! section every `edit_project` result carries (plan P04).
//!
//! A model that only hears "accepted" will declare success on a dark
//! strip. So after its edits land, the agent reads what the editor would
//! show: each node's status, each output port's endpoint with the pin it
//! resolves to on the project's board (or why it does not), and whether
//! unsaved edits remain. Compact on purpose — it rides every edit result;
//! the full detail is the `read` tool's job.

use lpc_model::{HwEndpointSpec, LpValue};
use serde_json::{Value, json};

use super::agent_project_edits::node_display_name;
use super::node::NodeController;
use super::project_node_tree_view::ProjectNodeStatusTone;
use crate::ProjectController;

impl ProjectController {
    /// What the patch surface has selected, for the agent's readout.
    pub(crate) fn agent_selection(&self) -> Option<String> {
        self.patch_selection_single()
            .map(|target| format!("{target:?}").chars().take(120).collect())
    }

    /// The project as the agent's edit results report it.
    pub(crate) fn agent_project_summary(&self) -> Value {
        let board = self
            .active_target()
            .and_then(|id| lpa_boards::board_by_id(&id).map(|board| (id, board)));
        let mut nodes = Vec::new();
        let mut outputs = Vec::new();
        let mut stack: Vec<&NodeController> = self.root_nodes().iter().collect();
        while let Some(node) = stack.pop() {
            stack.extend(node.children().iter().rev());
            if node.address().path().0.len() <= 1 {
                continue; // the project root module
            }
            let name = node_display_name(node.address());
            let status = node.status();
            let mut row = json!({
                "node": name,
                "kind": node.kind(),
                "status": status_word(status.tone),
            });
            if let Some(detail) = &status.detail {
                row["message"] = detail.clone().into();
            }
            nodes.push(row);
            if node.kind().eq_ignore_ascii_case("output") {
                let ports: Vec<Value> = output_endpoints(node)
                    .into_iter()
                    .map(|(port, endpoint)| {
                        port_row(
                            port,
                            &endpoint,
                            board.as_ref().map(|(id, board)| (id.as_str(), *board)),
                        )
                    })
                    .collect();
                outputs.push(json!({ "node": name, "ports": ports }));
            }
        }
        json!({
            "board": board.as_ref().map(|(id, _)| id.clone()),
            "nodes": nodes,
            "outputs": outputs,
            "unsaved": crate::has_unsaved_work(&self.dirty_summary()),
        })
    }
}

fn status_word(tone: ProjectNodeStatusTone) -> &'static str {
    match tone {
        ProjectNodeStatusTone::Good => "ok",
        ProjectNodeStatusTone::Warning => "warn",
        ProjectNodeStatusTone::Error => "error",
        ProjectNodeStatusTone::Fault => "fault",
        ProjectNodeStatusTone::Neutral | ProjectNodeStatusTone::Disabled => "pending",
    }
}

/// `(port key, endpoint)` for every authored port of an Output node, read
/// from the synced slot tree (`ports[<k>].endpoint`).
fn output_endpoints(node: &NodeController) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut stack: Vec<&super::slot::SlotController> = node.slots().iter().collect();
    while let Some(slot) = stack.pop() {
        stack.extend(slot.children().iter());
        let path = slot.address().path.to_string();
        let Some(rest) = path.strip_prefix("ports[") else {
            continue;
        };
        let Some((port, field)) = rest.split_once(']') else {
            continue;
        };
        if field != ".endpoint" {
            continue;
        }
        if let Some(LpValue::String(endpoint)) = slot.value() {
            out.push((port.to_string(), endpoint.clone()));
        }
    }
    out.sort();
    out
}

/// One port: its endpoint, and the pin it names on the project's board —
/// or why it names none.
fn port_row(
    port: String,
    endpoint: &str,
    board: Option<(&str, &lpa_boards::BoardDisplayFile)>,
) -> Value {
    let mut row = json!({ "port": port, "endpoint": endpoint });
    let spec = match HwEndpointSpec::parse(endpoint.to_string()) {
        Ok(spec) => spec,
        Err(_) => {
            row["problem"] = "the endpoint does not parse (expected ws281x:local:<pin>)".into();
            return row;
        }
    };
    let Some((board_id, board)) = board else {
        row["problem"] = "the project has no board, so the pin label means nothing yet".into();
        return row;
    };
    let label = spec.config();
    match board
        .output_wires()
        .find(|(wire, _)| wire.eq_ignore_ascii_case(label))
    {
        Some((_, gpio)) => row["pin"] = format!("/gpio/{gpio}").into(),
        None => {
            row["problem"] = format!(
                "{label} is not an LED output on {board_id} (outputs: {})",
                board
                    .output_wires()
                    .map(|(wire, _)| wire.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
            .into();
        }
    }
    row
}
