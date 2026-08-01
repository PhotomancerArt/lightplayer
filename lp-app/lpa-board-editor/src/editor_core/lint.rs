//! Lint rules for board display defs: every finding at once, not the first
//! error.
//!
//! [`lpa_boards::BoardDisplayFile::validate`] is the load/CI gate — it fails
//! fast on the first structural problem. The editor wants the whole picture
//! while the def is mid-edit, plus authoring guidance `validate` deliberately
//! doesn't enforce (missing price, unverified usb_bridge, geometry outside
//! the board). Error-level rules are a superset of `validate`'s checks, which
//! the parity test at the bottom pins.

use std::collections::BTreeMap;

use lpa_boards::{BoardDisplayFile, CapKind, PinRole};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum LintLevel {
    /// Blocks a def from being checked in (validate/CI would reject it, or a
    /// wrong-GPIO class of mistake).
    Error,
    /// Worth fixing before sharing; nothing breaks.
    Warn,
    /// Authoring context (summaries, unverified-field reminders).
    Info,
}

impl LintLevel {
    pub fn css_suffix(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warn => "warn",
            Self::Info => "info",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LintFinding {
    pub level: LintLevel,
    pub message: String,
}

/// Every finding for the def as it stands, errors first.
pub fn lint_board(board: &BoardDisplayFile) -> Vec<LintFinding> {
    let mut findings = Vec::new();
    let error = |message: String| LintFinding {
        level: LintLevel::Error,
        message,
    };
    let warn = |message: String| LintFinding {
        level: LintLevel::Warn,
        message,
    };
    let info = |message: String| LintFinding {
        level: LintLevel::Info,
        message,
    };

    // ---- identity / commerce --------------------------------------------
    if board.board_id.split('/').count() != 2
        || board.board_id.split('/').any(|part| part.trim().is_empty())
    {
        findings.push(error(format!(
            "board_id must be vendor/product, got {:?}",
            board.board_id
        )));
    }
    for (field, value) in [
        ("display_name", &board.display_name),
        ("manufacturer", &board.manufacturer),
        ("soc", &board.soc),
        ("family", &board.family),
        ("flash", &board.flash),
    ] {
        if value.trim().is_empty() {
            findings.push(if field == "display_name" {
                error(format!("{field} must not be empty"))
            } else {
                warn(format!("{field} is empty"))
            });
        }
    }
    if !board.price_usd.is_finite() || board.price_usd < 0.0 {
        findings.push(error(format!(
            "price_usd must be non-negative: {}",
            board.price_usd
        )));
    } else if board.price_usd == 0.0 {
        findings.push(warn(
            "price_usd is 0 — the catalog sorts by price; fill in the street price".into(),
        ));
    }
    if board.blurb.trim().is_empty() {
        findings.push(warn("blurb is empty — the catalog card shows it".into()));
    }
    if board.purchase_urls.is_empty() {
        findings.push(warn(
            "no purchase_urls — the catalog card has nowhere to send buyers".into(),
        ));
    }
    for url in &board.purchase_urls {
        if !(url.href.starts_with("https://") || url.href.starts_with("http://")) {
            findings.push(error(format!("purchase url must be http(s): {}", url.href)));
        }
        if url.label.trim().is_empty() {
            findings.push(warn(format!("purchase url {} has no label", url.href)));
        }
    }
    if board.usb_bridge.is_none() {
        findings.push(info(
            "usb_bridge unset — correct until the bridge chip is verified \
             (silkscreen, vendor docs, or a real enumeration); driver warnings \
             stay off until then"
                .into(),
        ));
    }

    // ---- pins: duplicates, label/gpio agreement, role/cap consistency ----
    let mut by_gpio: BTreeMap<u8, Vec<String>> = BTreeMap::new();
    let rail_pins = board
        .pins()
        .map(|pin| (pin.label.clone(), pin.role, pin.gpio, pin.caps.clone()));
    let terminals = board
        .hw
        .terminals
        .iter()
        .map(|terminal| {
            (
                terminal.label.clone(),
                terminal.role,
                terminal.gpio,
                terminal.caps.clone(),
            )
        });
    for (label, role, gpio, caps) in rail_pins.chain(terminals) {
        if label.trim().is_empty() {
            findings.push(error("pin label must not be empty".into()));
        }
        if let Some(gpio) = gpio {
            by_gpio.entry(gpio).or_default().push(label.clone());
        }
        // Numeric silkscreen labels ARE the gpio number; a mismatch (or a
        // missing gpio) is a physical-damage class of authoring mistake.
        if let Ok(numeric) = label.parse::<u8>() {
            if gpio != Some(numeric) {
                findings.push(error(format!(
                    "pin labeled {label:?} must carry gpio {numeric}, got {gpio:?}"
                )));
            }
        } else if let Some(numeric) = label
            .strip_prefix("IO")
            .and_then(|rest| rest.parse::<u8>().ok())
        {
            // Same rule for IO-prefixed silkscreens ("IO18"): the number is
            // the gpio claim.
            match gpio {
                Some(gpio) if gpio != numeric => findings.push(error(format!(
                    "pin labeled {label:?} claims gpio {numeric} but carries gpio {gpio}"
                ))),
                None => findings.push(warn(format!(
                    "pin labeled {label:?} names gpio {numeric} but carries none — \
                     set it or rename the pin"
                ))),
                Some(_) => {}
            }
        }
        let has_cap = |kind: CapKind| caps.iter().any(|cap| cap.kind == kind);
        if role == PinRole::Usb && !has_cap(CapKind::Usb) {
            findings.push(warn(format!(
                "pin {label}: role usb but no usb capability cell naming D+/D-"
            )));
        }
        if role != PinRole::Usb && has_cap(CapKind::Usb) {
            findings.push(warn(format!(
                "pin {label}: usb capability cell on a non-usb role ({role:?}) — \
                 driving USB pins drops the link; role usb marks that"
            )));
        }
        if role != PinRole::Strap && role != PinRole::Rsvd && has_cap(CapKind::Strap) {
            findings.push(warn(format!(
                "pin {label}: strap capability cell but role {role:?} — \
                 boot-strap pins use role strap"
            )));
        }
        for cap in &caps {
            if cap.text.trim().is_empty() {
                findings.push(warn(format!("pin {label}: capability cell with no text")));
            }
        }
    }
    for (gpio, labels) in &by_gpio {
        if labels.len() > 1 {
            findings.push(error(format!(
                "gpio {gpio} appears on {} pins: {}",
                labels.len(),
                labels.join(", ")
            )));
        }
    }
    if board.hw.left.is_empty() && board.hw.right.is_empty() {
        findings.push(warn("no rail pins — the diagram will be empty".into()));
    }

    // ---- drawing geometry sanity ----------------------------------------
    let width = board.hw.width;
    let module = &board.hw.module;
    if module.x < 0.0 || module.x + module.w > width {
        findings.push(warn(format!(
            "module ({}..{}) extends past the board width ({width})",
            module.x,
            module.x + module.w
        )));
    }
    for usb in &board.hw.usb {
        if usb.x < 0.0 || usb.x > width {
            findings.push(warn(format!(
                "usb connector {:?} at x={} is outside the board width ({width})",
                usb.label, usb.x
            )));
        }
    }
    for button in &board.hw.buttons {
        if button.x < 0.0 || button.x > width {
            findings.push(warn(format!(
                "button {:?} at x={} is outside the board width ({width})",
                button.label, button.x
            )));
        }
    }

    // ---- summaries -------------------------------------------------------
    let eligible = board
        .pins()
        .filter(|pin| pin.role.output_eligible() && pin.gpio.is_some())
        .count();
    let eligible_terminals = board
        .hw
        .terminals
        .iter()
        .filter(|terminal| terminal.role.output_eligible() && terminal.gpio.is_some())
        .count();
    findings.push(info(format!(
        "{} discovery-eligible pins (io/strap with a gpio){}",
        eligible + eligible_terminals,
        if eligible_terminals > 0 {
            format!(" — {eligible_terminals} on terminals")
        } else {
            String::new()
        }
    )));

    findings.sort_by_key(|finding| finding.level);
    findings
}

/// Errors only — what stands between this def and check-in.
pub fn lint_errors(board: &BoardDisplayFile) -> Vec<LintFinding> {
    lint_board(board)
        .into_iter()
        .filter(|finding| finding.level == LintLevel::Error)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor_core::editor_doc::EditorDoc;

    /// Every checked-in def lints clean at error level, and the lint rules
    /// stay a superset of `validate`: anything validate rejects must produce
    /// at least one Error finding.
    #[test]
    fn checked_in_boards_have_no_error_findings() {
        for board in lpa_boards::all_boards() {
            let errors = lint_errors(board);
            assert!(
                errors.is_empty(),
                "{}: {:?}",
                board.board_id,
                errors.iter().map(|f| &f.message).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn validate_rejections_surface_as_error_findings() {
        let mut doc = EditorDoc::new_template();
        // Seed the classes validate() rejects: bad id, duplicate gpio,
        // numeric-label mismatch.
        doc.edit(|board| {
            board.board_id = "no-slash".into();
            board.hw.right = board.hw.left.clone();
            board.hw.right.push(lpa_boards::DrawnPin {
                label: "5".into(),
                role: PinRole::Io,
                gpio: Some(6),
                caps: vec![],
            });
        });
        assert!(doc.board.validate().is_err());
        let errors = lint_errors(&doc.board);
        assert!(
            errors.len() >= 3,
            "expected id + duplicate + label findings, got {errors:?}"
        );
    }

    #[test]
    fn io_prefixed_labels_must_match_their_gpio() {
        let mut doc = EditorDoc::new_template();
        doc.edit(|board| {
            board.hw.left[0].label = "IO18".into();
            board.hw.left[0].gpio = Some(4);
        });
        let errors = lint_errors(&doc.board);
        assert!(
            errors.iter().any(|f| f.message.contains("IO18")),
            "{errors:?}"
        );
        // Matching gpio is clean; missing gpio is a warn, not an error.
        doc.edit(|board| board.hw.left[0].gpio = Some(18));
        assert!(lint_errors(&doc.board).is_empty());
        doc.edit(|board| board.hw.left[0].gpio = None);
        assert!(lint_errors(&doc.board).is_empty());
        assert!(
            lint_board(&doc.board)
                .iter()
                .any(|f| f.level == LintLevel::Warn && f.message.contains("IO18"))
        );
    }

    #[test]
    fn usb_role_and_cap_want_each_other() {
        let mut doc = EditorDoc::new_template();
        doc.edit(|board| board.hw.left[0].role = PinRole::Usb);
        let findings = lint_board(&doc.board);
        assert!(
            findings
                .iter()
                .any(|f| f.level == LintLevel::Warn && f.message.contains("role usb")),
            "{findings:?}"
        );
    }

    #[test]
    fn eligibility_summary_counts_io_and_strap_with_gpio() {
        let doc = EditorDoc::new_template();
        let findings = lint_board(&doc.board);
        assert!(
            findings
                .iter()
                .any(|f| f.level == LintLevel::Info
                    && f.message.starts_with("1 discovery-eligible")),
            "{findings:?}"
        );
    }
}
