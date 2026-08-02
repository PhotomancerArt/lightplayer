//! The failed-operation report: everything about a card's failure as one
//! pasteable block.
//!
//! When a heavy op fails, the card shows the error and its console tail —
//! enough to read, but painful to relay. Reproducing a failure for an
//! agent or a bug report means retyping the error, the device's state, the
//! chip, the board choice, the running build, and the log lines around it.
//! This builds that whole context in one copy.
//!
//! Deliberately plain text, not JSON: the destinations are a chat prompt
//! and a GitHub issue body, both of which read text better than they read
//! braces. It is a pure function of the card, so it is unit-tested rather
//! than eyeballed.

use lpa_studio_core::{CardOp, CardOpPhase, UiDeviceCard, UiLogEntry};

/// Studio's own version, for "which build produced this report".
const STUDIO_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Build the pasteable failure report for `card`'s failed op. Returns
/// `None` when the op is not in its failed phase — the button that calls
/// this only renders there, and this keeps that honest.
pub(crate) fn failure_report(card: &UiDeviceCard, op: &CardOp) -> Option<String> {
    let CardOpPhase::Failed { error, .. } = &op.phase else {
        return None;
    };
    let mut out = String::new();

    out.push_str("LightPlayer — operation failed\n");
    out.push_str("==============================\n");
    push_kv(&mut out, "operation", &op.label);
    push_kv(&mut out, "error", error);

    out.push_str("\ndevice\n");
    push_kv(&mut out, "  name", &card.name);
    push_kv(
        &mut out,
        "  uid",
        card.uid.as_deref().unwrap_or("(unstamped)"),
    );
    push_kv(&mut out, "  state", &format!("{:?}", card.state));
    push_kv(&mut out, "  transport", none_if_empty(&card.transport));
    push_kv(
        &mut out,
        "  detected chip",
        card.detected_chip.as_deref().unwrap_or("(not detected)"),
    );
    // The setup form's PICK and the device's own REPORT are different
    // facts, and a mismatch between them is exactly the kind of thing a
    // report should preserve rather than reconcile.
    push_kv(
        &mut out,
        "  board (picked)",
        card.ui
            .setup_board
            .as_deref()
            .unwrap_or("(generic — no pick)"),
    );
    push_kv(
        &mut out,
        "  board (reported)",
        card.hardware
            .as_ref()
            .and_then(|hardware| hardware.board_id.as_deref())
            .unwrap_or("(none)"),
    );
    if let Some(project) = card.project.as_ref() {
        push_kv(&mut out, "  project", &project.name);
    }
    if let Some(clamp) = card.safe_clamp {
        push_kv(&mut out, "  safe-mode clamp", &clamp.to_string());
    }

    match card.fw.as_ref() {
        Some(fw) => {
            out.push_str("\nfirmware on the device\n");
            push_kv(&mut out, "  package", &fw.package);
            push_kv(
                &mut out,
                "  commit",
                &format!("{}{}", fw.commit, if fw.dirty { " (dirty)" } else { "" }),
            );
            push_kv(&mut out, "  profile", &fw.profile);
            let features = fw
                .features
                .iter()
                .map(|feature| format!("{feature:?}"))
                .collect::<Vec<_>>()
                .join(", ");
            push_kv(&mut out, "  features", none_if_empty(&features));
        }
        // Absence is evidence here: no hello means the device never came
        // up far enough to introduce itself.
        None => out.push_str("\nfirmware on the device: (no hello — not running LightPlayer)\n"),
    }

    out.push_str("\nstudio\n");
    push_kv(&mut out, "  version", STUDIO_VERSION);
    push_kv(
        &mut out,
        "  served firmware",
        &lpa_link::SERVED_FIRMWARE_BUILDS.join(", "),
    );

    if card.console_tail.is_empty() {
        out.push_str("\nconsole: (empty)\n");
    } else {
        out.push_str(&format!(
            "\nconsole — last {} lines, oldest first\n",
            card.console_tail.len()
        ));
        for entry in &card.console_tail {
            out.push_str(&format!("  {}\n", console_line(entry)));
        }
    }
    Some(out)
}

fn push_kv(out: &mut String, key: &str, value: &str) {
    out.push_str(&format!("{key}: {value}\n"));
}

fn none_if_empty(value: &str) -> &str {
    if value.trim().is_empty() {
        "(none)"
    } else {
        value
    }
}

/// One console line: level, origin (+ its display detail when present),
/// message — the same facts the Console tab shows, flattened.
fn console_line(entry: &UiLogEntry) -> String {
    let origin = match entry.source.detail.as_deref() {
        Some(detail) => format!("{:?}/{detail}", entry.source.origin),
        None => format!("{:?}", entry.source.origin),
    };
    format!("[{:?}] {origin} {}", entry.level, entry.message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpa_studio_core::{CardOp, RosterCardState};

    fn failed_card() -> (UiDeviceCard, CardOp) {
        let card = UiDeviceCard {
            uid: None,
            name: "Luna's porch sign".to_string(),
            transport: "USB".to_string(),
            state: RosterCardState::ReadyToSetUp,
            project: None,
            fw: None,
            hardware: None,
            detected_chip: Some("esp32c6".to_string()),
            safe_clamp: None,
            sim: false,
            console_tail: Vec::new(),
            ui: lpa_studio_core::CardUiState {
                setup_board: Some("seeed/xiao-esp32-c6".to_string()),
                ..Default::default()
            },
        };
        let op = CardOp::failed(
            "Flashing firmware",
            "Failed to execute 'open' on 'SerialPort'",
            "Back to set up",
        );
        (card, op)
    }

    #[test]
    fn report_carries_the_operation_and_the_error() {
        let (card, op) = failed_card();
        let report = failure_report(&card, &op).expect("failed op reports");
        assert!(report.contains("operation: Flashing firmware"));
        assert!(report.contains("Failed to execute 'open' on 'SerialPort'"));
    }

    /// The point of the button: the context a human would otherwise have
    /// to reconstruct by hand.
    #[test]
    fn report_carries_the_device_context() {
        let (card, op) = failed_card();
        let report = failure_report(&card, &op).unwrap();
        for expected in [
            "Luna's porch sign",
            "(unstamped)",
            "USB",
            "esp32c6",
            "seeed/xiao-esp32-c6",
            "no hello",
            STUDIO_VERSION,
        ] {
            assert!(report.contains(expected), "missing {expected}:\n{report}");
        }
    }

    /// An unpicked board and a device that never said hello are FACTS, not
    /// blanks — a report full of empty values wastes the paste.
    #[test]
    fn absent_facts_read_as_explicit_absences() {
        let (mut card, op) = failed_card();
        card.ui.setup_board = None;
        card.detected_chip = None;
        card.transport = String::new();
        let report = failure_report(&card, &op).unwrap();
        assert!(report.contains("(generic — no pick)"));
        assert!(report.contains("(not detected)"));
        assert!(report.contains("(none)"));
        assert!(report.contains("console: (empty)"));
    }

    #[test]
    fn a_running_op_has_no_report() {
        let (card, _) = failed_card();
        assert!(failure_report(&card, &CardOp::new("Flashing firmware", None)).is_none());
        assert!(failure_report(&card, &CardOp::awaiting("Restarting…")).is_none());
    }
}
