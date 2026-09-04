//! The board · chip · MAC · firmware identity line the card header prints
//! (device-card-v2 plan, D2/AC3), and the chip join it is built on.
//!
//! In core rather than in the renderer for the usual reason — these are
//! decisions, and decisions get tests. `device_chip` is the one join every
//! other decision in this module (and `device_flash`'s offers) reads: a
//! board that hello'd names its own chip on the boot banner; a board that
//! was already running when the tab attached carries only its board id, so
//! the chip comes from the catalog's family for that id instead.

use lpa_devices::view::DeviceView;

/// The chip family for a device, normalized to the espflash vocabulary the
/// board-pick filter already uses ("esp32c6"): the banner when there is
/// one, else the family the hello's board id resolves to in the catalog.
///
/// `DeviceView::detected_chip` already carries its own fallback chain (the
/// banner, then the hello's firmware-package name, then the record) — this
/// join adds the LAST rung: a hello-only board whose banner was never seen
/// (an already-running board the tab attached to) still names a board id,
/// and the catalog's `family` for that id is the chip by another name.
pub fn device_chip(view: &DeviceView) -> Option<String> {
    view.detected_chip.clone().or_else(|| {
        view.board_id
            .as_deref()
            .and_then(lpa_boards::board_by_id)
            .map(|board| board.family.clone())
    })
}

/// The header's identity line: board · chip · MAC · firmware.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceIdentityLine {
    /// The board's catalog display name when its id resolves, else the raw
    /// id verbatim. `None` when no board id is known at all.
    pub board: Option<String>,
    /// The joined chip family ([`device_chip`]).
    pub chip: Option<String>,
    /// The strongest identity binding — [`DeviceView::identity_label`],
    /// read from the same source the card already prints under the title.
    pub mac: Option<String>,
    /// The settled hello's raw firmware label, when there was one. Unlike
    /// the other three fields this is never omitted from [`Self::display`]:
    /// a board with nothing to report reads "no firmware" rather than
    /// dropping the clause.
    pub firmware: Option<String>,
}

impl DeviceIdentityLine {
    /// The present parts, joined with " · ". Firmware always renders —
    /// "fw …" or the honest "no firmware" — because "nothing on this chip
    /// yet" is itself the fact the header is stating.
    pub fn display(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if let Some(board) = &self.board {
            parts.push(board.clone());
        }
        if let Some(chip) = &self.chip {
            parts.push(chip.clone());
        }
        if let Some(mac) = &self.mac {
            parts.push(mac.clone());
        }
        parts.push(match &self.firmware {
            Some(firmware) => format!("fw {firmware}"),
            None => "no firmware".to_string(),
        });
        parts.join(" · ")
    }
}

/// Project a device's identity line straight off its view.
pub fn device_identity_line(view: &DeviceView) -> DeviceIdentityLine {
    DeviceIdentityLine {
        board: view.board_id.as_deref().map(resolved_board_name),
        chip: device_chip(view),
        mac: view.identity_label.clone(),
        firmware: view.firmware.clone(),
    }
}

/// A board id's catalog display name, or the raw id when it does not
/// resolve — an id worth showing beats silence, even one the catalog does
/// not recognize (a build running an older/newer board list, say).
fn resolved_board_name(board_id: &str) -> String {
    lpa_boards::board_by_id(board_id)
        .map(|board| board.display_name.clone())
        .unwrap_or_else(|| board_id.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpa_devices::device::DeviceStatus;
    use lpa_devices::identity::DeviceId;
    use lpa_devices::view::{Escape, LoadedProject};

    /// A bare, mostly-empty card to fill in per test.
    fn card() -> DeviceView {
        DeviceView {
            id: DeviceId(1),
            title: "Bench board".to_string(),
            status: DeviceStatus::Ready,
            state_label: "Ready".to_string(),
            detail: None,
            freshness_label: None,
            identity_label: None,
            detected_chip: None,
            board_id: None,
            firmware: None,
            needs_firmware: false,
            degraded: None,
            loaded_project: LoadedProject::Empty,
            can_receive_project: true,
            can_remove_project: false,
            activity: None,
            last_outcome: None,
            terminal: Vec::new(),
            terminal_dropped: 0,
            escapes: vec![Escape::Forget],
        }
    }

    #[test]
    fn the_banner_wins_over_the_hello_boards_family() {
        let mut view = card();
        view.detected_chip = Some("esp32s3".to_string());
        view.board_id = Some("seeed/xiao-esp32-c6".to_string());
        assert_eq!(device_chip(&view).as_deref(), Some("esp32s3"));
    }

    #[test]
    fn a_hello_only_board_resolves_its_chip_from_its_board_id() {
        let mut view = card();
        view.detected_chip = None;
        view.board_id = Some("seeed/xiao-esp32-c6".to_string());
        assert_eq!(device_chip(&view).as_deref(), Some("esp32c6"));
    }

    #[test]
    fn an_unknown_board_id_resolves_no_chip() {
        let mut view = card();
        view.board_id = Some("nobody/nothing".to_string());
        assert_eq!(device_chip(&view), None);
    }

    #[test]
    fn no_sources_resolve_no_chip() {
        assert_eq!(device_chip(&card()), None);
    }

    #[test]
    fn the_full_identity_line_joins_every_part() {
        let mut view = card();
        view.board_id = Some("seeed/xiao-esp32-c6".to_string());
        view.detected_chip = Some("esp32c6".to_string());
        view.identity_label = Some("60:55:f9:0a:0b:0c".to_string());
        view.firmware = Some("fw-esp32c6 abc1234".to_string());

        let line = device_identity_line(&view);
        assert_eq!(line.board.as_deref(), Some("XIAO ESP32-C6"));
        assert_eq!(line.chip.as_deref(), Some("esp32c6"));
        assert_eq!(line.mac.as_deref(), Some("60:55:f9:0a:0b:0c"));
        assert_eq!(line.firmware.as_deref(), Some("fw-esp32c6 abc1234"));
        assert_eq!(
            line.display(),
            "XIAO ESP32-C6 · esp32c6 · 60:55:f9:0a:0b:0c · fw fw-esp32c6 abc1234"
        );
    }

    #[test]
    fn a_blank_board_reads_no_firmware_rather_than_omitting_the_clause() {
        let mut view = card();
        view.board_id = Some("seeed/xiao-esp32-c6".to_string());
        view.detected_chip = Some("esp32c6".to_string());
        view.firmware = None;

        let line = device_identity_line(&view);
        assert_eq!(line.firmware, None);
        assert_eq!(line.display(), "XIAO ESP32-C6 · esp32c6 · no firmware");
    }

    #[test]
    fn a_missing_mac_is_omitted_rather_than_shown_empty() {
        let mut view = card();
        view.board_id = Some("seeed/xiao-esp32-c6".to_string());
        view.detected_chip = Some("esp32c6".to_string());
        view.identity_label = None;
        view.firmware = Some("fw-esp32c6 abc1234".to_string());

        let display = device_identity_line(&view).display();
        assert!(!display.contains(" ·  ·"), "{display}");
        assert_eq!(display, "XIAO ESP32-C6 · esp32c6 · fw fw-esp32c6 abc1234");
    }

    /// An unresolvable board id still names itself, verbatim, rather than
    /// going silent.
    #[test]
    fn an_unresolvable_board_id_falls_back_to_the_raw_id() {
        let mut view = card();
        view.board_id = Some("nobody/nothing".to_string());

        assert_eq!(
            device_identity_line(&view).board.as_deref(),
            Some("nobody/nothing")
        );
    }
}
