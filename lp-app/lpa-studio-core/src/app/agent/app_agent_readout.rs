//! The app agent's readout: what the user sees right now, compact, sent with
//! every user turn and after every tool round (PD3, D9; focus v1 = A8).
//!
//! A dedicated projection of the core view model, never the DOM: the page,
//! the open project (its board, whether it is saved, each node's status and
//! where each output port lands), what is selected, the devices on the
//! roster, and the actions the view offers. Actions get short ids (`a1`,
//! `a2`, …) minted when the agent reads the readout (PD4): `UiAction` has no
//! stable id, so an id means "the action that was at this place in the
//! readout you were shown", and the host checks it against the latest
//! offers before dispatching (plan P08).

use std::fmt::Write as _;

use serde_json::Value;

use crate::{ActionEnablement, DeviceRosterView, UiAction};

/// The readout as the controller builds it after a batch: the text, and the
/// offered actions in display order (ids are minted on read).
#[derive(Clone, Debug, Default)]
pub struct AppReadoutSnapshot {
    pub text: String,
    pub actions: Vec<UiAction>,
}

/// One minted id and the action it names.
#[derive(Clone, Debug)]
pub struct MintedAction {
    pub id: String,
    pub action: UiAction,
}

impl AppReadoutSnapshot {
    /// The readout text with its actions listed under freshly minted ids,
    /// and the id table. The same snapshot always mints the same ids in the
    /// same order.
    pub fn mint(&self) -> (String, Vec<MintedAction>) {
        let mut text = self.text.clone();
        let minted: Vec<MintedAction> = self
            .actions
            .iter()
            .enumerate()
            .map(|(index, action)| MintedAction {
                id: format!("a{}", index + 1),
                action: action.clone(),
            })
            .collect();
        if minted.is_empty() {
            text.push_str("actions: none offered\n");
        } else {
            text.push_str("actions (ids are valid until the next readout):\n");
            for entry in &minted {
                let meta = entry.action.meta();
                let _ = write!(text, "- {}: {}", entry.id, meta.label);
                if let ActionEnablement::Disabled { reason } = &meta.enablement {
                    let _ = write!(text, " [disabled: {reason}]");
                }
                if meta.confirmation.is_some() {
                    text.push_str(" [asks the user to confirm]");
                }
                text.push('\n');
            }
        }
        (text, minted)
    }
}

/// The page line.
pub fn page_line(home: bool) -> String {
    if home {
        "page: home (no project open)\n".to_string()
    } else {
        "page: project editor\n".to_string()
    }
}

/// The open project, from its compact summary (`agent_project_summary`).
pub fn project_lines(name: &str, summary: &Value) -> String {
    let mut text = String::new();
    let board = summary["board"]
        .as_str()
        .map(|board| {
            format!(
                "{board} ({})",
                crate::app::roster::board_display_name(board)
            )
        })
        .unwrap_or_else(|| "none chosen".to_string());
    let unsaved = if summary["unsaved"].as_bool().unwrap_or(false) {
        "yes"
    } else {
        "no"
    };
    let _ = writeln!(
        text,
        "project: {name:?}; board: {board}; unsaved edits: {unsaved}"
    );
    let nodes = summary["nodes"].as_array().cloned().unwrap_or_default();
    if nodes.is_empty() {
        text.push_str("nodes: none (an empty project)\n");
    } else {
        text.push_str("nodes:\n");
        for node in nodes {
            let _ = write!(
                text,
                "- {} ({}) {}",
                node["node"].as_str().unwrap_or("?"),
                node["kind"].as_str().unwrap_or("?"),
                node["status"].as_str().unwrap_or("?"),
            );
            if let Some(message) = node["message"].as_str() {
                let _ = write!(text, ": {message}");
            }
            text.push('\n');
        }
    }
    for output in summary["outputs"].as_array().into_iter().flatten() {
        for port in output["ports"].as_array().into_iter().flatten() {
            let _ = write!(
                text,
                "- output {} port {} → {}",
                output["node"].as_str().unwrap_or("?"),
                port["port"].as_str().unwrap_or("?"),
                port["endpoint"].as_str().unwrap_or("?")
            );
            if let Some(pin) = port["pin"].as_str() {
                let _ = write!(text, " ({pin})");
            }
            if let Some(problem) = port["problem"].as_str() {
                let _ = write!(text, " — problem: {problem}");
            }
            text.push('\n');
        }
    }
    text
}

/// The selection line, when something is selected.
pub fn selection_line(selection: Option<String>) -> String {
    match selection {
        Some(selection) => format!("selected: {selection}\n"),
        None => String::new(),
    }
}

/// The device roster, one row per device.
pub fn device_lines(roster: &DeviceRosterView) -> String {
    let devices = &roster.roster.devices;
    if devices.is_empty() {
        return "devices: none connected\n".to_string();
    }
    let mut text = "devices:\n".to_string();
    for device in devices {
        let firmware = match &device.firmware_face {
            lpa_devices::FirmwareFace::LightPlayer { .. } => "LightPlayer",
            lpa_devices::FirmwareFace::Unknown => "not identified yet",
            _ => "other firmware",
        };
        let _ = writeln!(
            text,
            "- {:?}: chip {}; board {}; {firmware}; {}",
            device.title,
            device.detected_chip.as_deref().unwrap_or("unknown"),
            device.board_id.as_deref().unwrap_or("unknown"),
            device.state_label,
        );
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ControllerId, ProjectOp};

    fn snapshot() -> AppReadoutSnapshot {
        AppReadoutSnapshot {
            text: "page: project editor\n".to_string(),
            actions: vec![
                UiAction::from_op(ControllerId::new("studio|project"), ProjectOp::SaveOverlay),
                UiAction::from_op(
                    ControllerId::new("studio|project"),
                    ProjectOp::RevertAllEdits,
                )
                .disabled("nothing to revert"),
            ],
        }
    }

    #[test]
    fn the_same_view_mints_the_same_ids_in_the_same_order() {
        let (text_a, minted_a) = snapshot().mint();
        let (text_b, minted_b) = snapshot().mint();
        assert_eq!(text_a, text_b);
        let ids: Vec<&str> = minted_a.iter().map(|entry| entry.id.as_str()).collect();
        assert_eq!(ids, ["a1", "a2"]);
        assert!(
            minted_a
                .iter()
                .zip(&minted_b)
                .all(|(a, b)| a.id == b.id && a.action == b.action)
        );
        assert!(text_a.contains("- a2: "), "{text_a}");
        assert!(text_a.contains("[disabled: nothing to revert]"), "{text_a}");
    }

    #[test]
    fn a_stale_id_is_detectable() {
        // The table the agent saw names an action the next view no longer
        // offers: comparing actions, not ids, catches it.
        let (_, seen) = snapshot().mint();
        let mut next = snapshot();
        next.actions.remove(0);
        let (_, fresh) = next.mint();
        let saved = &seen[0].action;
        assert!(!fresh.iter().any(|entry| &entry.action == saved));
        assert_eq!(
            fresh[0].id, "a1",
            "ids are positional: a1 now names another action"
        );
    }
}
