//! The board · chip · MAC · firmware identity line the card header prints
//! (device-card-v2 plan, D2/AC3), and the chip join it is built on.
//!
//! In core rather than in the renderer for the usual reason — these are
//! decisions, and decisions get tests. `device_chip` is the one join every
//! other decision in this module (and `device_flash`'s offers) reads: a
//! board that hello'd names its own chip on the boot banner; a board that
//! was already running when the tab attached carries only its board id, so
//! the chip comes from the catalog's family for that id instead.

use lpa_devices::view::{DeviceView, FirmwareFace, PendingLinkView};

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

/// The header's identity line: board · chip · MAC · firmware — printed as
/// TWO fixed rows ([`Self::rows`]) since the follow-up to PR #514: at the
/// card's 400px column one truncated line ellipsised every card at
/// "… · fw fw-esp…", hiding the firmware clause exactly on the card it
/// exists for (the attached-but-closed classic, bench 2026-09-04). Spike:
/// `spikes/device-card-identity-line/index.html`, treatment E.
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
    /// The firmware clause. Unlike the other three fields this is never
    /// omitted from [`Self::display`]: a board with nothing reported and
    /// nothing remembered reads "no firmware" rather than dropping the
    /// clause.
    pub firmware: IdentityFirmware,
}

/// The firmware clause of the identity line: what this window's hello
/// reported, else what the record remembers, else nothing.
///
/// The Firmware ZONE states only the current window's verdict (ADR
/// 2026-09-04, "facts are stated when reported"). The header's identity
/// line is identity, not verdict — and a known board's identity includes
/// what it last ran. So when this window has no verdict at all (an
/// attached-but-closed port, a freshly rehydrated row, a board gone quiet)
/// the line reads the record's memory, and says it is memory: "… · last
/// seen". A window that HAS a verdict about the flash — blank,
/// bootloader, foreign, pre-hello — outranks the memory, so the line reads
/// "no firmware" there rather than naming a label the board no longer
/// wears (bench 2026-09-04: Disconnect on a known classic read
/// "no firmware" a second after "fw-esp32v3 7c80a27").
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IdentityFirmware {
    /// This window's hello named it.
    Reported(String),
    /// No verdict this window; the record remembers what the board last
    /// reported.
    Remembered(String),
    /// Nothing reported and nothing remembered — or a verdict that there
    /// is no LightPlayer on the flash.
    None,
}

impl IdentityFirmware {
    /// The label, whether reported or remembered.
    pub fn label(&self) -> Option<&str> {
        match self {
            Self::Reported(label) | Self::Remembered(label) => Some(label),
            Self::None => None,
        }
    }

    /// The clause's words: the label bare, or the honest "no firmware".
    /// Bare because every firmware label already starts with `fw-` — the
    /// old "fw fw-esp32v3 …" paid twice for one fact, on the line that had
    /// the least room to spare.
    pub fn label_text(&self) -> &str {
        match self {
            Self::Reported(label) | Self::Remembered(label) => label,
            Self::None => "no firmware",
        }
    }

    /// The memory mark that follows the label when it is what the record
    /// remembers rather than what this window heard. Its own clause so the
    /// renderer can set it in the dim tone — it stays plain text in the
    /// same line, selectable and pasteable with the rest.
    pub fn memory_mark(&self) -> Option<&'static str> {
        match self {
            Self::Remembered(_) => Some("last seen"),
            Self::Reported(_) | Self::None => None,
        }
    }
}

/// The identity line's two fixed rows: what the hardware is, and what is
/// bound to and running on it.
///
/// This split — board · chip above, MAC · firmware below — rather than
/// hardware (board · chip · MAC) above firmware alone, because at the
/// card's 400px column it is the one that holds every catalog board name:
/// "WLED LAN 4-Channel (DOM-Z-102) · esp32 · 30:76:f5:ec:f6:34" is 59
/// mono characters against ~56 of room, while each of these rows stays
/// under 50 (spike round 1, longest-board case).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IdentityRows {
    /// board · chip — empty when neither is known; the chip alone when
    /// only it is; the board alone when its name already states the chip
    /// ([`DeviceIdentityLine::chip_clause`]).
    pub board: String,
    /// MAC · firmware label (the label always; the MAC when known). The
    /// memory mark is NOT in here — [`IdentityFirmware::memory_mark`]
    /// hands it to the renderer separately so it can wear the dim tone.
    pub firmware: String,
}

impl IdentityRows {
    /// Both rows as one " · "-joined string — an empty board row drops out
    /// rather than leaving a leading separator. The header's `title` reads
    /// this, so hovering a truncated row shows the whole identity.
    pub fn display(&self) -> String {
        [self.board.as_str(), self.firmware.as_str()]
            .into_iter()
            .filter(|row| !row.is_empty())
            .collect::<Vec<_>>()
            .join(" · ")
    }
}

impl DeviceIdentityLine {
    /// The chip clause row 1 actually prints: the joined chip, unless the
    /// board name already states it.
    ///
    /// "XIAO ESP32-C6 · esp32c6" and "ESP32-S3-DevKitC-1 · esp32s3" pay
    /// twice for one fact on the row with the least room to spare;
    /// "QuinLED-Dig-Uno · esp32" does not, because the name says nothing
    /// about the chip. The rule (spike `device-card-identity-line`, the
    /// "drop chip when the board name says it" toggle): lowercase the
    /// board name, strip everything but letters and digits, and drop the
    /// chip when that string contains the chip family — so the name's
    /// "ESP32-C6" matches the family's "esp32c6" across the dash. A banner
    /// that names a DIFFERENT chip than the board name ("XIAO ESP32-C6"
    /// booting as esp32s3) keeps its chip: then the two are not one fact.
    pub fn chip_clause(&self) -> Option<&str> {
        let chip = self.chip.as_deref()?;
        match self.board.as_deref() {
            Some(board) if board_name_states_chip(board, chip) => None,
            _ => Some(chip),
        }
    }

    /// The two rows the header prints ([`IdentityRows`]).
    pub fn rows(&self) -> IdentityRows {
        let board = [self.board.as_deref(), self.chip_clause()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" · ");
        let firmware = [self.mac.as_deref(), Some(self.firmware.label_text())]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" · ");
        IdentityRows { board, firmware }
    }

    /// Every present part joined with " · " as one string — the rows, then
    /// the memory mark — for the header's `title` and for tests. Firmware
    /// always renders — the label, "… · last seen" or the honest "no
    /// firmware" — because "nothing on this chip yet" is itself the fact
    /// the header is stating.
    pub fn display(&self) -> String {
        let mut display = self.rows().display();
        if let Some(mark) = self.firmware.memory_mark() {
            display.push_str(" · ");
            display.push_str(mark);
        }
        display
    }
}

/// The row-1 words for a pending link whose boot banner named no chip (a
/// board parked in ROM download mode, or one that has said nothing).
pub const PENDING_CHIP_UNKNOWN: &str = "chip unknown";

/// The row-2 words for a pending link nothing has bound yet: identity and
/// firmware arrive together, at the flash.
pub const PENDING_NO_IDENTITY: &str = "no identity until flashed";

/// The pending card's two identity rows, in the settled card's grammar
/// (hardware above, binding · firmware below) so a link still identifying
/// reads like the cards beside it instead of one sentence in a two-row
/// slot (follow-up filed at the ship of PR #518).
///
/// A pending link has no board of record and, by construction, no
/// LightPlayer firmware — a hello promotes it to a device — so each row
/// says what THIS stage of identification honestly knows:
///
/// | stage | row 1 | row 2 |
/// |---|---|---|
/// | nothing heard | `chip unknown` | `no identity until flashed` |
/// | chip off the banner | `esp32c6` | `no identity until flashed` |
/// | chip + a probed MAC | `esp32c6` | `60:55:f9:0a:0b:0c · no firmware` |
///
/// Row 1 is the chip alone, exactly what the settled card prints for a
/// pre-hello board; it never goes blank, because an empty first row on a
/// card labelled Identifying reads as a rendering fault. Row 2 is the
/// settled `MAC · no firmware` the moment a MAC is known (the preflight
/// probe on a blank board is the one source), and until then the one
/// sentence that covers both missing clauses.
pub fn pending_identity_rows(pending: &PendingLinkView) -> IdentityRows {
    let board = pending
        .detected_chip
        .clone()
        .unwrap_or_else(|| PENDING_CHIP_UNKNOWN.to_string());
    let firmware = match pending.mac.as_deref() {
        Some(mac) => format!("{mac} · {}", IdentityFirmware::None.label_text()),
        None => PENDING_NO_IDENTITY.to_string(),
    };
    IdentityRows { board, firmware }
}

/// Project a device's identity line straight off its view.
pub fn device_identity_line(view: &DeviceView) -> DeviceIdentityLine {
    DeviceIdentityLine {
        board: view.board_id.as_deref().map(resolved_board_name),
        chip: device_chip(view),
        mac: view.identity_label.clone(),
        firmware: identity_firmware(view),
    }
}

/// The firmware clause: reported by this window's hello, else remembered
/// by the record when this window holds no verdict about the flash.
fn identity_firmware(view: &DeviceView) -> IdentityFirmware {
    if let Some(firmware) = view.firmware_face.firmware() {
        return IdentityFirmware::Reported(firmware.to_string());
    }
    // Unknown = no verdict yet (closed port, fresh row); Silent = the board
    // said nothing, which is no statement about its flash either. Every
    // other face IS a statement, and the memory yields to it.
    let window_is_silent = matches!(
        view.firmware_face,
        FirmwareFace::Unknown | FirmwareFace::Silent
    );
    match (&view.remembered_firmware, window_is_silent) {
        (Some(firmware), true) => IdentityFirmware::Remembered(firmware.clone()),
        _ => IdentityFirmware::None,
    }
}

/// Whether a board name already states the chip family: the name
/// lowercased and stripped to letters and digits contains the family
/// ("xiaoesp32c6" contains "esp32c6"; "quinleddiguno" does not contain
/// "esp32"). See [`DeviceIdentityLine::chip_clause`].
fn board_name_states_chip(board: &str, chip: &str) -> bool {
    let flat: String = board
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect();
    let chip: String = chip.to_ascii_lowercase();
    !chip.is_empty() && flat.contains(&chip)
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
            firmware_face: lpa_devices::view::FirmwareFace::Unknown,
            remembered_firmware: None,
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

    /// A board whose name says nothing about its chip prints every
    /// clause: board · chip above, MAC · firmware below.
    #[test]
    fn the_full_identity_line_joins_every_part() {
        let mut view = card();
        view.board_id = Some("quinled/dig-uno".to_string());
        view.detected_chip = Some("esp32".to_string());
        view.identity_label = Some("30:76:f5:ec:f6:34".to_string());
        view.firmware_face = lpa_devices::view::FirmwareFace::LightPlayer {
            firmware: Some("fw-esp32v3 7c80a27".to_string()),
            wire: lpa_devices::WireVersion::Match,
        };

        let line = device_identity_line(&view);
        assert_eq!(line.board.as_deref(), Some("QuinLED-Dig-Uno"));
        assert_eq!(line.chip.as_deref(), Some("esp32"));
        assert_eq!(line.chip_clause(), Some("esp32"));
        assert_eq!(line.mac.as_deref(), Some("30:76:f5:ec:f6:34"));
        assert_eq!(
            line.firmware,
            IdentityFirmware::Reported("fw-esp32v3 7c80a27".to_string())
        );
        assert_eq!(
            line.rows(),
            IdentityRows {
                board: "QuinLED-Dig-Uno · esp32".to_string(),
                firmware: "30:76:f5:ec:f6:34 · fw-esp32v3 7c80a27".to_string(),
            }
        );
        assert_eq!(
            line.display(),
            "QuinLED-Dig-Uno · esp32 · 30:76:f5:ec:f6:34 · fw-esp32v3 7c80a27"
        );
    }

    /// The follow-up to PR #518: a board whose catalog name already
    /// states the chip prints the name alone on row 1 — the chip stays
    /// joined on the line (every other decision still reads it), it just
    /// does not print twice. Both catalog spellings of the fact are
    /// covered: "ESP32-C6" against `esp32c6`, "ESP32-S3-DevKitC-1"
    /// against `esp32s3`. `display()` is the rows joined, so it agrees.
    #[test]
    fn a_board_name_that_states_the_chip_drops_the_chip_clause() {
        for (board_id, name, chip) in [
            ("seeed/xiao-esp32-c6", "XIAO ESP32-C6", "esp32c6"),
            (
                "espressif/esp32-s3-devkitc-1",
                "ESP32-S3-DevKitC-1",
                "esp32s3",
            ),
            (
                "espressif/esp32-c6-devkitc-1",
                "ESP32-C6-DevKitC-1",
                "esp32c6",
            ),
        ] {
            let mut view = card();
            view.board_id = Some(board_id.to_string());
            view.detected_chip = Some(chip.to_string());
            view.identity_label = Some("60:55:f9:0a:0b:0c".to_string());
            view.firmware_face = lpa_devices::view::FirmwareFace::LightPlayer {
                firmware: Some("fw-esp32c6 abc1234".to_string()),
                wire: lpa_devices::WireVersion::Match,
            };

            let line = device_identity_line(&view);
            assert_eq!(line.chip.as_deref(), Some(chip), "{board_id}");
            assert_eq!(line.chip_clause(), None, "{board_id}");
            let rows = line.rows();
            assert_eq!(rows.board, name, "{board_id}");
            assert_eq!(rows.firmware, "60:55:f9:0a:0b:0c · fw-esp32c6 abc1234");
            assert_eq!(
                line.display(),
                format!("{name} · 60:55:f9:0a:0b:0c · fw-esp32c6 abc1234"),
                "{board_id}"
            );
        }
    }

    /// A board name that says nothing about the chip keeps the chip —
    /// including a name that merely contains a prefix of the family's
    /// letters somewhere unrelated (the DOM-Z-102's "4-Channel" is not
    /// "esp32").
    #[test]
    fn a_board_name_that_does_not_state_the_chip_keeps_it() {
        for (board_id, name) in [
            ("quinled/dig-uno", "QuinLED-Dig-Uno"),
            ("quinled/dig2go", "QuinLED-dig2go"),
            ("domraem/dom-z-102", "WLED LAN 4-Channel (DOM-Z-102)"),
        ] {
            let mut view = card();
            view.board_id = Some(board_id.to_string());
            view.detected_chip = Some("esp32".to_string());

            let line = device_identity_line(&view);
            assert_eq!(line.board.as_deref(), Some(name), "{board_id}");
            assert_eq!(line.chip_clause(), Some("esp32"), "{board_id}");
            assert_eq!(line.rows().board, format!("{name} · esp32"), "{board_id}");
        }
    }

    /// The name states ONE chip; a banner that names another is a second
    /// fact, not a repeat — an XIAO ESP32-C6 record whose boot banner
    /// reads esp32s3 keeps its chip on the row.
    #[test]
    fn a_banner_that_disagrees_with_the_board_name_keeps_the_chip() {
        let mut view = card();
        view.board_id = Some("seeed/xiao-esp32-c6".to_string());
        view.detected_chip = Some("esp32s3".to_string());

        let line = device_identity_line(&view);
        assert_eq!(line.chip_clause(), Some("esp32s3"));
        assert_eq!(line.rows().board, "XIAO ESP32-C6 · esp32s3");
    }

    /// The rule reads the string on the row, so an unresolved id that
    /// happens to spell its chip is deduplicated the same way — and one
    /// that does not is not.
    #[test]
    fn the_dedupe_reads_the_raw_id_when_the_board_does_not_resolve() {
        let mut view = card();
        view.board_id = Some("nobody/esp32-c6-thing".to_string());
        view.detected_chip = Some("esp32c6".to_string());
        assert_eq!(
            device_identity_line(&view).rows().board,
            "nobody/esp32-c6-thing"
        );

        view.board_id = Some("nobody/nothing".to_string());
        assert_eq!(
            device_identity_line(&view).rows().board,
            "nobody/nothing · esp32c6"
        );
    }

    #[test]
    fn a_blank_board_reads_no_firmware_rather_than_omitting_the_clause() {
        let mut view = card();
        view.board_id = Some("seeed/xiao-esp32-c6".to_string());
        view.detected_chip = Some("esp32c6".to_string());
        view.firmware_face = lpa_devices::view::FirmwareFace::Blank;

        let line = device_identity_line(&view);
        assert_eq!(line.firmware, IdentityFirmware::None);
        assert_eq!(line.display(), "XIAO ESP32-C6 · no firmware");
        assert_eq!(line.rows().firmware, "no firmware");
    }

    #[test]
    fn a_missing_mac_is_omitted_rather_than_shown_empty() {
        let mut view = card();
        view.board_id = Some("seeed/xiao-esp32-c6".to_string());
        view.detected_chip = Some("esp32c6".to_string());
        view.identity_label = None;
        view.firmware_face = lpa_devices::view::FirmwareFace::LightPlayer {
            firmware: Some("fw-esp32c6 abc1234".to_string()),
            wire: lpa_devices::WireVersion::Match,
        };

        let display = device_identity_line(&view).display();
        assert!(!display.contains(" ·  ·"), "{display}");
        assert_eq!(display, "XIAO ESP32-C6 · fw-esp32c6 abc1234");
    }

    /// A pre-hello board (a chip and nothing else) still prints both rows:
    /// the chip alone above, "no firmware" below — never a blank row 1
    /// with the chip shoved down.
    #[test]
    fn a_chip_only_board_keeps_the_chip_on_the_board_row() {
        let mut view = card();
        view.detected_chip = Some("esp32c6".to_string());
        view.firmware_face = lpa_devices::view::FirmwareFace::NoHello;

        let rows = device_identity_line(&view).rows();
        assert_eq!(rows.board, "esp32c6");
        assert_eq!(rows.firmware, "no firmware");
    }

    /// The bench case (2026-09-04): Disconnect on a known classic. The
    /// window restarts, so the face is Unknown — and chip and board already
    /// survive via the record. The firmware clause must survive the same
    /// way, marked as memory rather than passed off as live.
    #[test]
    fn a_closed_window_reads_the_remembered_firmware_marked_as_last_seen() {
        let mut view = card();
        view.board_id = Some("quinled/dig-uno".to_string());
        view.detected_chip = Some("esp32".to_string());
        view.identity_label = Some("30:76:f5:ec:f6:34".to_string());
        view.firmware_face = lpa_devices::view::FirmwareFace::Unknown;
        view.remembered_firmware = Some("fw-esp32v3 7c80a27".to_string());

        let line = device_identity_line(&view);
        assert_eq!(
            line.firmware,
            IdentityFirmware::Remembered("fw-esp32v3 7c80a27".to_string())
        );
        assert_eq!(line.firmware.label(), Some("fw-esp32v3 7c80a27"));
        assert_eq!(line.firmware.label_text(), "fw-esp32v3 7c80a27");
        assert_eq!(line.firmware.memory_mark(), Some("last seen"));
        // The mark rides beside the rows, not inside them, so the renderer
        // can dim it; the joined string carries it as the last clause.
        assert_eq!(
            line.rows(),
            IdentityRows {
                board: "QuinLED-Dig-Uno · esp32".to_string(),
                firmware: "30:76:f5:ec:f6:34 · fw-esp32v3 7c80a27".to_string(),
            }
        );
        assert_eq!(
            line.display(),
            "QuinLED-Dig-Uno · esp32 · 30:76:f5:ec:f6:34 · fw-esp32v3 7c80a27 · last seen"
        );

        // A board gone quiet has made no statement about its flash either.
        view.firmware_face = lpa_devices::view::FirmwareFace::Silent;
        assert_eq!(
            device_identity_line(&view).firmware,
            IdentityFirmware::Remembered("fw-esp32v3 7c80a27".to_string())
        );
    }

    /// This window's hello outranks the memory — and it is never marked as
    /// remembered, even when the two agree.
    #[test]
    fn a_reported_firmware_outranks_the_remembered_one() {
        let mut view = card();
        view.firmware_face = lpa_devices::view::FirmwareFace::LightPlayer {
            firmware: Some("fw-esp32v3 1111111".to_string()),
            wire: lpa_devices::WireVersion::Match,
        };
        view.remembered_firmware = Some("fw-esp32v3 0000000".to_string());

        let line = device_identity_line(&view);
        assert_eq!(
            line.firmware,
            IdentityFirmware::Reported("fw-esp32v3 1111111".to_string())
        );
        assert_eq!(line.firmware.memory_mark(), None);
        assert!(!line.display().contains("last seen"));
    }

    /// A verdict that there is no LightPlayer on the flash (erased, parked
    /// in ROM, somebody else's firmware, pre-hello) outranks the memory:
    /// naming a label the board no longer wears would be the lie the
    /// "· last seen" mark exists to avoid.
    #[test]
    fn a_flash_verdict_outranks_the_remembered_firmware() {
        use lpa_devices::view::FirmwareFace;
        for face in [
            FirmwareFace::Blank,
            FirmwareFace::Bootloader,
            FirmwareFace::NoHello,
            FirmwareFace::Foreign { label: None },
        ] {
            let mut view = card();
            view.firmware_face = face.clone();
            view.remembered_firmware = Some("fw-esp32v3 7c80a27".to_string());

            let line = device_identity_line(&view);
            assert_eq!(line.firmware, IdentityFirmware::None, "{face:?}");
            assert!(line.display().ends_with("no firmware"), "{face:?}");
        }
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

    /// A bare pending link to fill in per stage.
    fn pending() -> PendingLinkView {
        PendingLinkView {
            link: lpa_devices::LinkId(3),
            device: DeviceId(103),
            title: "New device".to_string(),
            state_label: "New device found — identifying…".to_string(),
            detail: None,
            can_adopt: true,
            firmware_face: FirmwareFace::Unknown,
            detected_chip: None,
            mac: None,
            escapes: vec![Escape::Forget],
        }
    }

    /// Nothing heard yet: neither row goes blank — a blank first row on a
    /// card labelled Identifying reads as a fault, not as "unknown".
    #[test]
    fn a_pending_link_with_nothing_known_fills_both_rows() {
        let rows = pending_identity_rows(&pending());
        assert_eq!(rows.board, "chip unknown");
        assert_eq!(rows.firmware, "no identity until flashed");
        assert_eq!(rows.display(), "chip unknown · no identity until flashed");
    }

    /// The banner named the chip: row 1 is the chip alone, exactly what the
    /// settled card prints for a pre-hello board, so the two line up.
    #[test]
    fn a_pending_link_with_a_chip_keeps_it_on_the_board_row() {
        let mut view = pending();
        view.detected_chip = Some("esp32c6".to_string());

        let rows = pending_identity_rows(&view);
        assert_eq!(rows.board, "esp32c6");
        assert_eq!(rows.firmware, "no identity until flashed");

        let mut settled = card();
        settled.detected_chip = Some("esp32c6".to_string());
        settled.firmware_face = FirmwareFace::NoHello;
        assert_eq!(device_identity_line(&settled).rows().board, rows.board);
    }

    /// A probed MAC makes row 2 the settled card's `MAC · no firmware` —
    /// the identity is bound now, and "until flashed" would be stale.
    #[test]
    fn a_pending_link_with_a_mac_reads_like_a_settled_blank_board() {
        let mut view = pending();
        view.detected_chip = Some("esp32c6".to_string());
        view.mac = Some("60:55:f9:0a:0b:0c".to_string());
        view.firmware_face = FirmwareFace::Blank;

        let rows = pending_identity_rows(&view);
        assert_eq!(rows.board, "esp32c6");
        assert_eq!(rows.firmware, "60:55:f9:0a:0b:0c · no firmware");
        assert!(!rows.display().contains("until flashed"));

        let mut settled = card();
        settled.detected_chip = Some("esp32c6".to_string());
        settled.identity_label = Some("60:55:f9:0a:0b:0c".to_string());
        settled.firmware_face = FirmwareFace::Blank;
        assert_eq!(device_identity_line(&settled).rows(), rows);
    }

    /// The settled verdict changes the Firmware zone's line, never the
    /// identity rows: a link with a chip and no MAC reads the same whether
    /// it is still identifying or settled blank.
    #[test]
    fn a_pending_links_rows_do_not_move_when_its_verdict_settles() {
        let mut view = pending();
        view.detected_chip = Some("esp32c6".to_string());
        let identifying = pending_identity_rows(&view);
        view.firmware_face = FirmwareFace::Blank;
        assert_eq!(pending_identity_rows(&view), identifying);
    }
}
