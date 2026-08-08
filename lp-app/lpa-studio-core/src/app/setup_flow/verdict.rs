//! R3 — what is on the board, decided once, as an enum.
//!
//! Design: `docs/design/device-setup-flow.md` §4. One probe pass yields one
//! [`BoardVerdict`] alongside `detected_chip`, replacing the implicit "did
//! hello answer" checks that used to be scattered across this seam.
//!
//! **The bias is asymmetric on purpose.** Under-claiming costs little: a
//! WLED board read as [`BoardVerdict::Blank`] only means the wipe offer is
//! skipped, and the flash confirmation still guards the data. Over-claiming
//! [`BoardVerdict::LightPlayer`] is not recoverable by any later guard — it
//! routes a stranger's board into the adopt branch. So a proto-matching
//! wire hello is the ONLY evidence that yields `LightPlayer`, and WLED
//! detection stays conservative (a banner match; ambiguous Improv traffic
//! alone is not enough).
//!
//! [`BoardVerdict::StaleLightPlayer`] is the one verdict that reads as
//! LightPlayer-ish and still must never BE `LightPlayer`: the link saw our
//! wire framing but no proto-matching hello, so the app protocol is not
//! available on this board. Adopting it would put an unreachable board on
//! the roster; its one affordance is a reflash.

use crate::app::places::{HardwareId, RegisteredDevice};

/// Everything one probe pass observed, in the vocabulary the existing link
/// machinery already produces.
///
/// `hello_seen` is `lpa_link`'s readiness gate (a `ServerHello` whose
/// `proto` matched); `no_firmware_signature` is
/// `BootLineClassifier::no_firmware_detected`; `bootloader_conversation` is
/// the escalation probe's own `DeviceLinkMode`; `lines` is the snapshot's
/// `recent_lines` tail. Nothing here is re-derived — this struct is a
/// carrier so the classification can be tested with no wire.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProbeEvidence {
    /// A proto-matching `ServerHello` arrived on this session.
    pub hello_seen: bool,
    /// A known no-firmware boot signature was observed (blank header, ROM
    /// download mode, or a known replaceable banner).
    pub no_firmware_signature: bool,
    /// The link classified this peer as `DeviceState::Incompatible`:
    /// LightPlayer-framing wire, no proto-matching hello. Old firmware, in
    /// practice — the board talks, this Studio just cannot talk to it.
    pub stale_lightplayer: bool,
    /// A ROM/stub bootloader ANSWERED the escalation probe's esptool SYNC
    /// handshake (`BootloaderEvidence::SyncHandshake`) — the strongest
    /// no-firmware-running evidence there is, since only a bootloader
    /// answers it.
    ///
    /// Separate from `no_firmware_signature` because it comes from a
    /// different place and nothing else can supply it: the snapshot's
    /// `link_mode` is recomputed PASSIVELY from a boot-line classifier that
    /// the probe's own link rebuild wipes clean, so a conversation that
    /// just succeeded is invisible one line later. Only the probe's return
    /// value carries it (bench, 2026-08-08: a board parked in the esptool
    /// stub answered the escalation and the wizard still said "nothing
    /// intelligible answered").
    pub bootloader_conversation: bool,
    /// The non-protocol serial tail (`DeviceSnapshot::recent_lines`).
    pub lines: Vec<String>,
    /// Chip identity as reported (`DeviceSnapshot::detected_chip`).
    pub detected_chip: Option<String>,
    /// The base MAC a download-mode read banked (`DeviceSnapshot::probed_mac`),
    /// or the hello's — whichever the caller gathered. Feeds the registry
    /// lookup and the provision-time registry write.
    pub base_mac: Option<String>,
}

/// What the board is running.
///
/// `known` is one registry lookup keyed by the probed MAC's derived uid:
/// it is what lets BOARD_PICK and WLED_FOUND say "was Porch sign".
/// `Unresponsive` carries the field for uniformity; a board that said
/// nothing intelligible has no MAC either, so in practice it is `None`.
///
/// `StaleLightPlayer` is LightPlayer firmware too old for this Studio to
/// talk to: recognised, never adopted (module doc).
#[derive(Debug, Clone, PartialEq)]
pub enum BoardVerdict {
    LightPlayer { known: Option<RegisteredDevice> },
    StaleLightPlayer { known: Option<RegisteredDevice> },
    Wled { known: Option<RegisteredDevice> },
    Blank { known: Option<RegisteredDevice> },
    Unresponsive { known: Option<RegisteredDevice> },
}

impl BoardVerdict {
    /// The remembered row this board matched, whatever the verdict.
    pub fn known(&self) -> Option<&RegisteredDevice> {
        match self {
            Self::LightPlayer { known }
            | Self::StaleLightPlayer { known }
            | Self::Wled { known }
            | Self::Blank { known }
            | Self::Unresponsive { known } => known.as_ref(),
        }
    }

    /// Stable label for event-log records and test tables.
    pub fn label(&self) -> &'static str {
        match self {
            Self::LightPlayer { .. } => "lightplayer",
            Self::StaleLightPlayer { .. } => "stale-lightplayer",
            Self::Wled { .. } => "wled",
            Self::Blank { .. } => "blank",
            Self::Unresponsive { .. } => "unresponsive",
        }
    }
}

/// One probe pass's whole answer: the verdict, the chip, and the identity
/// the flow writes the registry row under.
#[derive(Debug, Clone, PartialEq)]
pub struct BoardProbe {
    pub verdict: BoardVerdict,
    /// `DeviceSnapshot::detected_chip`, which filters the board pick.
    pub detected_chip: Option<String>,
    /// The registry key this unit resolved to (`HardwareId::device_uid`),
    /// when the evidence anchored one. `None` = anonymous board: nothing is
    /// remembered and nothing is written at provision.
    pub hardware_uid: Option<String>,
    /// The canonical origin string recorded on the registry row
    /// (`HardwareId`'s `Display`).
    pub hardware_origin: Option<String>,
}

/// Substrings that name WLED in a boot banner or an Improv service reply.
/// Deliberately one token: everything else observed on those wires is
/// shared with other firmwares.
const WLED_MARKERS: &[&str] = &["wled"];

/// Classify one probe pass. `registry` is the remembered-device list the
/// `known` lookup runs against — a slice, so this stays pure.
pub fn classify_board(evidence: &ProbeEvidence, registry: &[RegisteredDevice]) -> BoardProbe {
    let hardware_id = evidence
        .base_mac
        .as_deref()
        .and_then(HardwareId::from_base_mac);
    let known = hardware_id.and_then(|id| known_device_for(&id, registry));

    // Rule 1 and the whole point of the enum: a wire hello, and nothing
    // else, makes a board LightPlayer.
    let verdict = if evidence.hello_seen {
        BoardVerdict::LightPlayer { known }
    } else if evidence.stale_lightplayer {
        // The link's `Incompatible` classification: our framing on the
        // wire, no proto-matching hello. Strong evidence about WHAT the
        // board runs, and deliberately not `LightPlayer` — nothing the
        // Studio can talk to is there to adopt. It outranks the banner and
        // no-firmware rules below because it names the firmware exactly.
        BoardVerdict::StaleLightPlayer { known }
    } else if evidence.lines.iter().any(|line| is_wled_line(line)) {
        BoardVerdict::Wled { known }
    } else if evidence.no_firmware_signature || evidence.bootloader_conversation {
        // A bootloader that answered is Blank-class evidence: nothing this
        // Studio can talk to is running, and the board's affordance is the
        // pick-and-flash BOARD_PICK offers. This arm is what turns "it's
        // dead" into "it's blank, here are your boards" for a board the
        // passive read heard nothing from.
        BoardVerdict::Blank { known }
    } else {
        BoardVerdict::Unresponsive { known }
    };

    BoardProbe {
        verdict,
        detected_chip: evidence.detected_chip.clone(),
        hardware_uid: hardware_id.map(|id| id.device_uid().to_string()),
        hardware_origin: hardware_id.map(|id| id.to_string()),
    }
}

/// The remembered row for a resolved identity: by derived uid first, then
/// by the origin column (a row re-keyed under a different scheme still
/// records the silicon it came from).
pub fn known_device_for(
    hardware_id: &HardwareId,
    registry: &[RegisteredDevice],
) -> Option<RegisteredDevice> {
    let uid = hardware_id.device_uid().to_string();
    let origin = hardware_id.to_string();
    registry
        .iter()
        .find(|row| row.uid == uid)
        .or_else(|| {
            registry
                .iter()
                .find(|row| row.hardware_id.as_deref() == Some(origin.as_str()))
        })
        .or_else(|| {
            registry
                .iter()
                .find(|row| row.previous_uids.iter().any(|old| *old == uid))
        })
        .cloned()
}

fn is_wled_line(line: &str) -> bool {
    let lowered = line.to_ascii_lowercase();
    WLED_MARKERS.iter().any(|marker| lowered.contains(marker))
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAC: &str = "aa:bb:cc:dd:ee:ff";

    fn row(uid: &str, name: &str) -> RegisteredDevice {
        RegisteredDevice {
            uid: uid.to_string(),
            name: name.to_string(),
            transport: "USB".to_string(),
            last_seen_at: 1.0,
            association: None,
            board_id: None,
            hardware_id: None,
            previous_uids: Vec::new(),
        }
    }

    fn known_row(name: &str) -> RegisteredDevice {
        let uid = HardwareId::from_base_mac(MAC).unwrap().device_uid();
        row(&uid.to_string(), name)
    }

    #[test]
    fn only_a_hello_yields_lightplayer() {
        let summary = classify_board(
            &ProbeEvidence {
                hello_seen: true,
                ..ProbeEvidence::default()
            },
            &[],
        );
        assert!(matches!(summary.verdict, BoardVerdict::LightPlayer { .. }));
    }

    #[test]
    fn nothing_short_of_a_hello_is_ever_lightplayer() {
        // The one forbidden error. Every non-hello evidence shape must land
        // anywhere BUT LightPlayer, however LightPlayer-ish it reads.
        let shapes = [
            ProbeEvidence {
                lines: vec!["fw-esp32 initialized, starting server loop".to_string()],
                ..ProbeEvidence::default()
            },
            ProbeEvidence {
                lines: vec!["LightPlayer ready".to_string()],
                ..ProbeEvidence::default()
            },
            ProbeEvidence {
                no_firmware_signature: true,
                lines: vec!["lightplayer".to_string()],
                ..ProbeEvidence::default()
            },
            // The nearest miss there is: our own wire framing, and still
            // no hello. Old firmware is not a board to adopt.
            ProbeEvidence {
                stale_lightplayer: true,
                lines: vec!["LightPlayer ready".to_string()],
                ..ProbeEvidence::default()
            },
            ProbeEvidence::default(),
        ];
        for evidence in shapes {
            let summary = classify_board(&evidence, &[]);
            assert!(
                !matches!(summary.verdict, BoardVerdict::LightPlayer { .. }),
                "{evidence:?} must not be called LightPlayer"
            );
        }
    }

    #[test]
    fn an_incompatible_link_is_stale_lightplayer() {
        let summary = classify_board(
            &ProbeEvidence {
                stale_lightplayer: true,
                ..ProbeEvidence::default()
            },
            &[],
        );
        assert!(matches!(
            summary.verdict,
            BoardVerdict::StaleLightPlayer { .. }
        ));
    }

    #[test]
    fn a_named_firmware_outranks_a_banner_and_a_blank_signature() {
        // The link identified the firmware; the weaker rules below it must
        // not relabel the board as something to wipe blind or a dead port.
        for evidence in [
            ProbeEvidence {
                stale_lightplayer: true,
                lines: vec!["WLED 0.14.0 ready".to_string()],
                ..ProbeEvidence::default()
            },
            ProbeEvidence {
                stale_lightplayer: true,
                no_firmware_signature: true,
                ..ProbeEvidence::default()
            },
        ] {
            let summary = classify_board(&evidence, &[]);
            assert!(
                matches!(summary.verdict, BoardVerdict::StaleLightPlayer { .. }),
                "{evidence:?} names the firmware"
            );
        }
    }

    #[test]
    fn a_wled_banner_is_wled() {
        let summary = classify_board(
            &ProbeEvidence {
                lines: vec![
                    "ets Jul 29 2019 12:21:46".to_string(),
                    "WLED 0.14.0 ready".to_string(),
                ],
                ..ProbeEvidence::default()
            },
            &[],
        );
        assert!(matches!(summary.verdict, BoardVerdict::Wled { .. }));
    }

    #[test]
    fn wled_detection_is_conservative_not_eager() {
        // Improv traffic alone names no firmware. Reading this as Blank is
        // the acceptable error (§4): the flash warning still guards.
        let summary = classify_board(
            &ProbeEvidence {
                no_firmware_signature: true,
                lines: vec!["IMPROV service ready".to_string()],
                ..ProbeEvidence::default()
            },
            &[],
        );
        assert!(matches!(summary.verdict, BoardVerdict::Blank { .. }));
    }

    #[test]
    fn a_bootloader_that_answered_is_blank_never_unresponsive() {
        // The whole point of the escalation (§8): it runs ONLY on
        // `Unresponsive`, so evidence carrying its success must never come
        // back Unresponsive — that would offer BOOT-hold advice to a board
        // whose bootloader just held a conversation.
        let summary = classify_board(
            &ProbeEvidence {
                bootloader_conversation: true,
                detected_chip: Some("ESP32-D0WD-V3".to_string()),
                ..ProbeEvidence::default()
            },
            &[],
        );
        assert!(
            matches!(summary.verdict, BoardVerdict::Blank { .. }),
            "an answered SYNC handshake is blank-class evidence, got {:?}",
            summary.verdict
        );
        assert_eq!(summary.detected_chip.as_deref(), Some("ESP32-D0WD-V3"));
    }

    #[test]
    fn a_bootloader_that_answered_never_outranks_a_named_firmware() {
        // Ordering guard: the escalation cannot run once a firmware named
        // itself, but if both were ever present the named one still wins.
        for (evidence, label) in [
            (
                ProbeEvidence {
                    bootloader_conversation: true,
                    stale_lightplayer: true,
                    ..ProbeEvidence::default()
                },
                "stale-lightplayer",
            ),
            (
                ProbeEvidence {
                    bootloader_conversation: true,
                    lines: vec!["WLED 0.14.0 ready".to_string()],
                    ..ProbeEvidence::default()
                },
                "wled",
            ),
        ] {
            assert_eq!(classify_board(&evidence, &[]).verdict.label(), label);
        }
    }

    #[test]
    fn a_no_firmware_signature_is_blank() {
        let summary = classify_board(
            &ProbeEvidence {
                no_firmware_signature: true,
                lines: vec!["invalid header: 0xffffffff".to_string()],
                detected_chip: Some("esp32c6".to_string()),
                ..ProbeEvidence::default()
            },
            &[],
        );
        assert!(matches!(summary.verdict, BoardVerdict::Blank { .. }));
        assert_eq!(summary.detected_chip.as_deref(), Some("esp32c6"));
    }

    #[test]
    fn silence_is_unresponsive() {
        let summary = classify_board(&ProbeEvidence::default(), &[]);
        assert!(matches!(summary.verdict, BoardVerdict::Unresponsive { .. }));
        assert_eq!(summary.hardware_uid, None);
    }

    #[test]
    fn every_verdict_carries_the_registry_match_for_a_known_mac() {
        let registry = vec![known_row("Porch sign")];
        let cases = [
            (
                ProbeEvidence {
                    hello_seen: true,
                    base_mac: Some(MAC.to_string()),
                    ..ProbeEvidence::default()
                },
                "lightplayer",
            ),
            (
                ProbeEvidence {
                    stale_lightplayer: true,
                    base_mac: Some(MAC.to_string()),
                    ..ProbeEvidence::default()
                },
                "stale-lightplayer",
            ),
            (
                ProbeEvidence {
                    lines: vec!["WLED ready".to_string()],
                    base_mac: Some(MAC.to_string()),
                    ..ProbeEvidence::default()
                },
                "wled",
            ),
            (
                ProbeEvidence {
                    no_firmware_signature: true,
                    base_mac: Some(MAC.to_string()),
                    ..ProbeEvidence::default()
                },
                "blank",
            ),
            (
                ProbeEvidence {
                    base_mac: Some(MAC.to_string()),
                    ..ProbeEvidence::default()
                },
                "unresponsive",
            ),
        ];
        for (evidence, label) in cases {
            let summary = classify_board(&evidence, &registry);
            assert_eq!(summary.verdict.label(), label);
            assert_eq!(
                summary.verdict.known().map(|row| row.name.as_str()),
                Some("Porch sign"),
                "{label} must carry the recognition"
            );
            assert_eq!(
                summary.hardware_origin.as_deref(),
                Some("efuse:aa:bb:cc:dd:ee:ff")
            );
        }
    }

    #[test]
    fn an_unknown_mac_recognises_nothing() {
        let registry = vec![row("dev0000000000000001", "Someone else")];
        let summary = classify_board(
            &ProbeEvidence {
                no_firmware_signature: true,
                base_mac: Some(MAC.to_string()),
                ..ProbeEvidence::default()
            },
            &registry,
        );
        assert_eq!(summary.verdict.known(), None);
        assert!(
            summary.hardware_uid.is_some(),
            "the identity still resolves"
        );
    }

    #[test]
    fn recognition_follows_the_origin_column_and_previous_uids() {
        let mut by_origin = row("devlegacyrow00001", "By origin");
        by_origin.hardware_id = Some("efuse:aa:bb:cc:dd:ee:ff".to_string());
        let summary = classify_board(
            &ProbeEvidence {
                base_mac: Some(MAC.to_string()),
                no_firmware_signature: true,
                ..ProbeEvidence::default()
            },
            &[by_origin],
        );
        assert_eq!(
            summary.verdict.known().map(|row| row.name.as_str()),
            Some("By origin")
        );

        let derived = HardwareId::from_base_mac(MAC).unwrap().device_uid();
        let mut by_history = row("dev0000000000000009", "By history");
        by_history.previous_uids = vec![derived.to_string()];
        let summary = classify_board(
            &ProbeEvidence {
                base_mac: Some(MAC.to_string()),
                no_firmware_signature: true,
                ..ProbeEvidence::default()
            },
            &[by_history],
        );
        assert_eq!(
            summary.verdict.known().map(|row| row.name.as_str()),
            Some("By history")
        );
    }

    #[test]
    fn a_failed_efuse_read_anchors_no_identity() {
        // The all-ones read is what a FAILED efuse read looks like; it must
        // not collapse every failed board onto one remembered row.
        let summary = classify_board(
            &ProbeEvidence {
                base_mac: Some("ff:ff:ff:ff:ff:ff".to_string()),
                no_firmware_signature: true,
                ..ProbeEvidence::default()
            },
            &[known_row("Porch sign")],
        );
        assert_eq!(summary.hardware_uid, None);
        assert_eq!(summary.verdict.known(), None);
    }
}
