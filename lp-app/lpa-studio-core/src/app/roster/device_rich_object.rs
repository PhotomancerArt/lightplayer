//! The device as a rich object: evidence → the fixed section schema.
//!
//! [`device_rich_object`] builds the device's [`RichObjectView`] — the
//! section list behind the card's detail trigger — from the same derived
//! evidence the card already renders. The schema order is FIXED (Q4):
//! **Health, Project, Technical, Performance, Backup, Danger zone** —
//! users learn where things are. A section with no data is omitted, but
//! its schema slot never moves:
//!
//! | Section | tone source | weight | present when |
//! |---|---|---|---|
//! | Health | the card state's circle tone | Actionable | always |
//! | Project | drift (Attention on behind/diverged) | Actionable | a project is known (held now or last ran) |
//! | Technical | Neutral (+ advisory fw chip) | Advisory | uid / transport / hello fw evidence exists |
//! | Performance | Neutral/Attention | Advisory | never yet — `ProjectRuntimeSummary` does not flow to cards |
//! | Backup | Neutral | Actionable | a device copy was banked at connect (diverged) |
//! | Danger zone | Neutral (never colors rollup) | Danger | live manageable link → flash+erase; offline registered → forget |
//!
//! The Health section IS today's card derivation: its tone and affordance
//! come straight from [`RosterCardState`] (itself the product of
//! [`derive_roster_card_state`](super::derive_roster_card_state)), so the
//! popover can never disagree with the circle.

use lpc_model::{LpFeature, NodeKind};
use lpc_wire::{BuildFacts, HardwareFacts};

use crate::app::project::node::node_naming::node_kind_label;

use crate::app::rich_object::{RichChip, RichLine, RichObjectView, RichSection, RichWeight};
use crate::core::status::UiStatusKind;

use super::firmware_update::BundledFirmware;
use super::roster_affordance::RosterAffordance;
use super::roster_card_state::RosterCardState;

/// A device section's affordance identity. Wiring to concrete actions is
/// the renderer's job (the card layer already owns the roster-affordance
/// mapping); the danger verbs are identities for the same reason.
#[derive(Clone, Debug, PartialEq)]
pub enum DeviceDetailAffordance {
    /// A card-grammar affordance (per the direction state table).
    Roster(RosterAffordance),
    /// Danger zone, live device: download a ZIP of the device's storage,
    /// read raw over the bootloader. The non-destructive row that belongs
    /// ABOVE the destructive ones — it is what makes them survivable.
    BackUpFilesystem,
    /// Danger zone, live device: install/repair firmware.
    FlashFirmware,
    /// Danger zone, live device: wipe the flash (confirmed).
    EraseDevice,
    /// Danger zone, offline registered device: forget it (D34 hygiene).
    ForgetDevice,
    /// Danger zone, live device: close this board's session and drop its
    /// card (multi-device M3 — with several boards attachable there must
    /// be a way OUT per board; the board keeps running and reconnecting
    /// adds it back). Wired only when the card carries a session key.
    DisconnectDevice,
}

/// Everything the device builder may know, assembled from the card's
/// derived state plus the evidence the card view-model carries. Missing
/// evidence is honest evidence of absence: the section it feeds is
/// omitted.
#[derive(Clone, Debug, PartialEq)]
pub struct DeviceRichInput<'a> {
    /// The derived card state (via `derive_roster_card_state`).
    pub state: &'a RosterCardState,
    /// `dev_…` uid when registered/stamped.
    pub uid: Option<&'a str>,
    /// Transport label ("USB"); empty while a connect resolves.
    pub transport: &'a str,
    /// The project the device holds (live) or last ran (offline).
    pub project_name: Option<&'a str>,
    /// Running-firmware build facts from the hello (live links only):
    /// provenance plus the features compiled into the image.
    pub fw: Option<&'a BuildFacts>,
    /// What the unit has wired, from the same hello.
    pub hardware: Option<&'a HardwareFacts>,
    /// Studio's bundled firmware image, when the packaged manifest is on
    /// hand — the advisory chip comparison's other half.
    pub bundled_fw: Option<&'a BundledFirmware>,
    /// Chip identity from passive/probe evidence — a Technical line even
    /// before any firmware hello exists (gate-1 sitting, 2026-08-03: an
    /// unflashed card's Technical tab identified nothing but "USB").
    pub detected_chip: Option<&'a str>,
    /// The port as the app can name it (endpoint label + grant short id).
    /// Web Serial never exposes the OS path, so this is the whole truth.
    pub port_label: Option<&'a str>,
    /// f64 epoch seconds for status-line recency copy.
    pub now_secs: f64,
}

/// Build the device's rich-object view. Pure; the section table on the
/// module doc is normative.
pub fn device_rich_object(input: &DeviceRichInput<'_>) -> RichObjectView<DeviceDetailAffordance> {
    let mut sections = vec![health_section(input)];
    sections.extend(project_section(input));
    sections.extend(technical_section(input));
    // Performance: `ProjectRuntimeSummary` is typed but does not flow to
    // roster cards yet — the schema slot exists here, between Technical
    // and Backup, and fills when runtime stats arrive.
    sections.extend(backup_section(input));
    sections.extend(danger_section(input));
    RichObjectView::new(sections)
}

/// Health: the card state itself, as a section. Tone and affordance are
/// the card derivation's — one derivation, consumed everywhere.
fn health_section(input: &DeviceRichInput<'_>) -> RichSection<DeviceDetailAffordance> {
    let mut lines = vec![RichLine::new(
        "status",
        input.state.status_line(input.now_secs),
    )];
    // The sub-line rides along as a fact row — including the diverged
    // card's plain-words situation copy (§3a: the Status tab explains;
    // the Backup section still carries the banked facts).
    if let Some(sub_line) = input.state.sub_line(input.now_secs) {
        lines.push(RichLine::new("note", sub_line));
    }
    // The running family also offers the editor entry as a visible CTA
    // on the card face (2026-07-26 walk: the grow ⤢ alone was too easy
    // to miss — it stays, but the Status tab now says it out loud).
    // Running-up-to-date's STATE affordance already is the open — only
    // the drifted states need it added beside their Push/Review.
    let state_affordance = input.state.affordance();
    let open_editor = (matches!(
        input.state,
        RosterCardState::RunningUpToDate
            | RosterCardState::RunningBehind { .. }
            | RosterCardState::EditedOnDevice { .. }
    ) && state_affordance != Some(RosterAffordance::OpenEditor))
    .then_some(DeviceDetailAffordance::Roster(RosterAffordance::OpenEditor));
    // §3c-2: the diverged face carries BOTH verbs — Keep-both rides
    // beside the state's Use-board-copy, then the editor CTA.
    let keep_both = matches!(input.state, RosterCardState::EditedOnDevice { .. })
        .then_some(DeviceDetailAffordance::Roster(RosterAffordance::KeepBoth));
    RichSection {
        title: "Health".to_string(),
        tone: input.state.spec().tone,
        lines,
        chip: None,
        // The state-table affordance stays FIRST (it is the rollup's
        // primary — Push/Review must not be demoted); the editor CTA
        // rides second.
        affordances: state_affordance
            .map(DeviceDetailAffordance::Roster)
            .into_iter()
            .chain(keep_both)
            .chain(open_editor)
            .collect(),
        weight: RichWeight::Actionable,
    }
}

/// Project: what the device holds (live) or last ran (offline), with the
/// drift facts. Drift tone matches the card grammar (Attention on
/// behind/diverged). The section's own affordance is the D29 open —
/// push/resolve stay on Health per the state table, so the two sections
/// never offer the same verb twice.
fn project_section(input: &DeviceRichInput<'_>) -> Option<RichSection<DeviceDetailAffordance>> {
    let name = input.project_name?;
    let mut lines = Vec::new();
    let mut tone = UiStatusKind::Neutral;
    let mut affordances = Vec::new();
    match input.state {
        RosterCardState::RunningBehind {
            observed_version,
            head_version,
        } => {
            tone = UiStatusKind::Attention;
            lines.push(RichLine::new("running", name.to_string()));
            // §3a: the drift distance in plain words (saves, not vN).
            // History records no wall-clock yet — edit-time copy lands
            // with the timestamp plumbing (model backlog).
            let distance = match (observed_version, head_version) {
                (Some(observed), Some(head)) if *head > *observed => {
                    let behind = head - observed;
                    if behind == 1 {
                        "1 save behind your latest".to_string()
                    } else {
                        format!("{behind} saves behind your latest")
                    }
                }
                _ => "behind your latest".to_string(),
            };
            lines.push(RichLine::new("note", distance));
            affordances.push(DeviceDetailAffordance::Roster(RosterAffordance::OpenEditor));
        }
        RosterCardState::EditedOnDevice { .. } => {
            tone = UiStatusKind::Attention;
            lines.push(RichLine::new(
                "running",
                format!("{name} · changed on the device"),
            ));
            affordances.push(DeviceDetailAffordance::Roster(RosterAffordance::OpenEditor));
        }
        RosterCardState::RunningUpToDate => {
            lines.push(RichLine::new("running", format!("{name} · up to date")));
            affordances.push(DeviceDetailAffordance::Roster(RosterAffordance::OpenEditor));
        }
        RosterCardState::Offline { .. } => {
            lines.push(RichLine::new("last ran", name.to_string()));
        }
        // Other states (working, provisioning family, …) may still carry a
        // last-known chip: identity, not drift.
        _ => {
            lines.push(RichLine::new("project", name.to_string()));
        }
    }
    Some(RichSection {
        title: "Project".to_string(),
        tone,
        lines,
        chip: None,
        affordances,
        weight: RichWeight::Actionable,
    })
}

/// Technical: identity and provenance facts (advisory — never colors the
/// rollup), plus the standing firmware-update chip when both comparison
/// sides are honestly known.
fn technical_section(input: &DeviceRichInput<'_>) -> Option<RichSection<DeviceDetailAffordance>> {
    let mut lines = Vec::new();
    if let Some(uid) = input.uid {
        lines.push(RichLine::new("uid", uid));
    }
    // Everything knowable about an unprovisioned board rides here too
    // (gate-1 sitting, 2026-08-03): before any firmware hello the tab
    // showed only "transport USB", which identified nothing — with two
    // boards attached, nothing on screen said which was which beyond the
    // title. Web Serial never exposes the OS port path or manufacturer
    // strings, so chip (boot banner / probe) + endpoint label (VID:PID +
    // grant id) is the complete honest set.
    if let Some(chip) = input.detected_chip {
        // The silicon revision comes from the device's own efuse and only
        // exists after a hello; `detected_chip` is the boot banner, which
        // is all an unflashed board can offer. Combine them when both are
        // known rather than spending two lines on one fact.
        match input.hardware.and_then(|hw| hw.chip_revision.as_deref()) {
            Some(revision) => {
                lines.push(RichLine::new("chip", format!("{chip} · rev {revision}")));
            }
            None => lines.push(RichLine::new("chip", chip)),
        }
    }
    if let Some(port) = input.port_label {
        lines.push(RichLine::new("port", port));
    }
    if !input.transport.is_empty() {
        lines.push(RichLine::new("transport", input.transport));
    }
    if let Some(fw) = input.fw {
        let dirty = if fw.dirty { " (dirty)" } else { "" };
        lines.push(RichLine::new(
            "firmware",
            format!("{} @ {}{dirty} · {}", fw.package, fw.commit, fw.profile),
        ));
    }
    // The chip's OWN identity, from efuse. Worth its own line above the
    // capability gaps: unlike everything else here it is permanent — it
    // survives an erase, which the `dev_…` uid does not, because that one
    // lives in the device's filesystem.
    if let Some(hardware) = input.hardware {
        if let Some(mac) = hardware.base_mac.as_deref() {
            lines.push(RichLine::new("mac", mac));
        }
        // 802.15.4 (Zigbee/Thread) — 64 bits, and only on parts that have
        // that radio, so its absence is not a gap worth reporting.
        if let Some(eui64) = hardware.eui64.as_deref() {
            lines.push(RichLine::new("eui-64", eui64));
        }
    }
    lines.extend(capability_lines(input.fw, input.hardware));
    if lines.is_empty() {
        return None;
    }
    let chip = input
        .bundled_fw
        .zip(input.fw)
        .filter(|(bundled, fw)| bundled.update_available(fw))
        .map(|_| RichChip {
            tone: UiStatusKind::Attention,
            text: "Firmware update available".to_string(),
        });
    Some(RichSection {
        title: "Technical".to_string(),
        tone: UiStatusKind::Neutral,
        lines,
        chip,
        affordances: Vec::new(),
        weight: RichWeight::Advisory,
    })
}

/// Backup: what banking knows. Today that is the D8 connect-time bank of
/// a diverged device copy; a download affordance lands with the flow that
/// can serve it (no dead buttons).
/// The device's capabilities as Technical lines — **gaps only**.
///
/// A fully-capable device says nothing extra: listing everything a normal
/// board has would bury the one line that matters on the board that lacks
/// something. So each line here reports an ABSENCE (or a backend that is
/// not the norm), and a device with no gaps contributes no lines at all.
fn capability_lines(build: Option<&BuildFacts>, hardware: Option<&HardwareFacts>) -> Vec<RichLine> {
    let mut lines = Vec::new();
    let Some(build) = build else {
        return lines;
    };

    // Node kinds whose runtime this build does not carry. Ungated kinds
    // (`for_node_kind` → None) are always present and never listed.
    let missing: Vec<&'static str> = NodeKind::ALL
        .iter()
        .filter(|kind| {
            LpFeature::for_node_kind(**kind)
                .is_some_and(|feature| !build.features.contains(&feature))
        })
        .map(|kind| node_kind_label(*kind))
        .collect();
    if !missing.is_empty() {
        lines.push(RichLine::new("no nodes", missing.join(" · ")));
    }

    // A graphics backend worth naming: the norm (the CPU shader backend)
    // stays silent; "no shaders at all" and "GPU" do not.
    if build.features.contains(&LpFeature::GfxNull) {
        lines.push(RichLine::new(
            "graphics",
            "none — this build runs no shaders",
        ));
    } else if build.features.contains(&LpFeature::GfxWgpu) {
        lines.push(RichLine::new("graphics", "GPU (wgpu)"));
    }

    if let Some(hardware) = hardware {
        let mut absent = Vec::new();
        if !hardware.radio {
            absent.push("radio");
        }
        if !hardware.button {
            absent.push("button");
        }
        if !absent.is_empty() {
            lines.push(RichLine::new("no hardware", absent.join(" · ")));
        }
        if let Some(board_id) = &hardware.board_id {
            lines.push(RichLine::new("board", board_id.clone()));
        }
    }

    lines
}

fn backup_section(input: &DeviceRichInput<'_>) -> Option<RichSection<DeviceDetailAffordance>> {
    matches!(input.state, RosterCardState::EditedOnDevice { .. }).then(|| RichSection {
        title: "Backup".to_string(),
        tone: UiStatusKind::Neutral,
        lines: vec![
            RichLine::new("banked", "Device copy saved to history"),
            RichLine::new("when", "At connect"),
        ],
        chip: None,
        affordances: Vec::new(),
        weight: RichWeight::Actionable,
    })
}

/// Danger zone, pinned last — and present in EVERY state (state-flow
/// model §3: the always-available short-circuit; its rows adapt). Live
/// manageable cards carry flash + erase; a blank/foreign board carries
/// erase (its MAIN action is the setup form's install, not danger); the
/// not-responding card carries the recovery flash + forget; remembered,
/// mid-connect, and port-held cards carry forget. The one exception is
/// mid-operation — the in-place overlay covers the tabs (§2).
fn danger_section(input: &DeviceRichInput<'_>) -> Option<RichSection<DeviceDetailAffordance>> {
    let forget = || {
        input
            .uid
            .is_some()
            .then_some(DeviceDetailAffordance::ForgetDevice)
    };
    // Troubleshoot is offered in EVERY state (2026-07-31). It used to hang
    // off NotResponding alone, which is the wrong gate: the states where a
    // user most needs the recovery flow are the ones where the ladder ended
    // somewhere else — a board in download mode presents as Recovery mode,
    // not as Not-responding, and had no path to it at all. The danger zone
    // is already present in every state, so it is the natural permanent
    // home for the recovery verbs.
    let troubleshoot = || DeviceDetailAffordance::Roster(RosterAffordance::Troubleshoot);
    let affordances = match input.state {
        RosterCardState::Offline { .. }
        | RosterCardState::ConnectingRetrying { .. }
        | RosterCardState::InUseElsewhere => forget().into_iter().collect(),
        RosterCardState::OperationInFlight { .. } => Vec::new(),
        // The setup form owns the install; erase is the short-circuit.
        // Disconnect closes the live session (multi-device M3: per-board
        // way out; the web wires it only when a session key exists).
        RosterCardState::ReadyToSetUp | RosterCardState::OtherFirmware => {
            vec![
                DeviceDetailAffordance::EraseDevice,
                DeviceDetailAffordance::DisconnectDevice,
            ]
        }
        RosterCardState::NotResponding => core::iter::once(DeviceDetailAffordance::FlashFirmware)
            .chain(forget())
            .chain(core::iter::once(DeviceDetailAffordance::DisconnectDevice))
            .collect(),
        _ => {
            let mut rows = Vec::new();
            // A live card holding a project can always wipe it back to
            // blank from here (2026-07-26 walk: a problematic project
            // must be removable without waiting for a sad state). Cards
            // that already LEAD with wipe on Health — unreadable content,
            // and a format this build cannot migrate — skip it here: one
            // offer, not two.
            //
            // The old-format card has no project chip (its content never
            // resolved to a running library project), so it is named
            // explicitly rather than through `project_name`: an upgradable
            // board still deserves the way out, in case the user would
            // rather start over than migrate.
            let holds_project = input.project_name.is_some()
                || matches!(input.state, RosterCardState::HoldsOldFormatProject { .. });
            let leads_with_wipe = match input.state {
                RosterCardState::HoldsUnreadableData { .. } => true,
                RosterCardState::HoldsOldFormatProject { standing, .. } => {
                    !standing.is_upgradable()
                }
                _ => false,
            };
            if holds_project && !leads_with_wipe {
                rows.push(DeviceDetailAffordance::Roster(
                    RosterAffordance::WipeProject,
                ));
            }
            rows.push(DeviceDetailAffordance::FlashFirmware);
            rows.push(DeviceDetailAffordance::EraseDevice);
            rows.push(DeviceDetailAffordance::DisconnectDevice);
            rows
        }
    };
    // "Always available" means "wherever the danger zone exists". Working
    // states deliberately have no danger zone at all — and troubleshooting
    // mid-operation would want the wire the operation is already holding —
    // so an empty list still means no section, not a section with one row.
    if affordances.is_empty() {
        return None;
    }
    // Backup rides above the destructive verbs wherever a filesystem could
    // be read (M6): the wire is attached, and the board has LightPlayer
    // storage on it.
    //
    // NOT on a blank or foreign board — their `lpfs` is erased or was never
    // written, so the row could only ever fail — and not on the offline /
    // retrying / port-held cards, which have no wire to read over.
    let can_back_up = !matches!(
        input.state,
        RosterCardState::ReadyToSetUp
            | RosterCardState::OtherFirmware
            | RosterCardState::Offline { .. }
            | RosterCardState::ConnectingRetrying { .. }
            | RosterCardState::InUseElsewhere
    );
    let backup = can_back_up.then_some(DeviceDetailAffordance::BackUpFilesystem);
    // Prepend: these are the non-destructive rows, above the destructive
    // verbs — and backup in particular is what makes those survivable, so it
    // must be READ before them, not found after.
    let affordances: Vec<_> = core::iter::once(troubleshoot())
        .chain(backup)
        .chain(affordances)
        .collect();
    Some(RichSection {
        title: "Danger zone".to_string(),
        // Neutral by construction: Danger weight never colors the rollup;
        // the renderer's inline-tinted treatment carries the red.
        tone: UiStatusKind::Neutral,
        lines: Vec::new(),
        chip: None,
        affordances,
        weight: RichWeight::Danger,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The chip's own efuse identity reaches the Technical tab (2026-08-03).
    ///
    /// The MAC matters beyond display: it is the only identity of a board
    /// that SURVIVES AN ERASE. The `dev_…` uid lives in the device
    /// filesystem and dies with it, so before this the card had no
    /// permanent way to say which physical board it was.
    #[test]
    fn the_technical_tab_reports_the_chips_own_identity() {
        let hardware = HardwareFacts {
            radio: true,
            button: true,
            base_mac: Some("60:55:f9:01:02:03".to_string()),
            chip_revision: Some("0.2".to_string()),
            eui64: Some("60:55:f9:ff:fe:01:02:03".to_string()),
            ..Default::default()
        };
        let state = RosterCardState::RunningUpToDate;
        let mut input = input(&state);
        input.fw = Some(&DEVICE_FW);
        input.hardware = Some(&hardware);
        input.detected_chip = Some("esp32c6");

        let view = device_rich_object(&input);
        let technical = view
            .sections
            .iter()
            .find(|section| section.title == "Technical")
            .expect("a Technical section");
        let lines: Vec<(&str, &str)> = technical
            .lines
            .iter()
            .map(|line| (line.label.as_str(), line.value.as_str()))
            .collect();

        assert!(lines.contains(&("mac", "60:55:f9:01:02:03")), "{lines:?}");
        assert!(
            lines.contains(&("eui-64", "60:55:f9:ff:fe:01:02:03")),
            "the 802.15.4 address is 64 bits and gets its own line: {lines:?}"
        );
        // The revision JOINS the chip line rather than taking its own —
        // one fact, one row.
        assert!(lines.contains(&("chip", "esp32c6 · rev 0.2")), "{lines:?}");
    }

    /// A board that has said nothing about itself yet (no hello, so no
    /// efuse read) reports no identity lines rather than empty ones.
    #[test]
    fn an_unflashed_board_reports_no_chip_identity_lines() {
        let state = RosterCardState::ConnectedEmpty;
        let mut input = input(&state);
        input.hardware = None;
        input.detected_chip = Some("esp32c6");

        let view = device_rich_object(&input);
        let technical = view
            .sections
            .iter()
            .find(|section| section.title == "Technical")
            .expect("a Technical section");
        let labels: Vec<&str> = technical
            .lines
            .iter()
            .map(|line| line.label.as_str())
            .collect();

        assert!(!labels.contains(&"mac"), "{labels:?}");
        assert!(!labels.contains(&"eui-64"), "{labels:?}");
        // …and the chip line stays bare, with no dangling revision.
        let chip = technical
            .lines
            .iter()
            .find(|line| line.label == "chip")
            .expect("the boot banner still identifies the chip");
        assert_eq!(chip.value, "esp32c6");
    }

    /// An UNFLASHED board's Technical tab identifies the device with
    /// everything Web Serial can honestly say — chip + port label — not
    /// just "transport USB" (gate-1 sitting, 2026-08-03: with two boards
    /// attached, nothing beyond the title said which was which).
    #[test]
    fn an_unflashed_board_gets_chip_and_port_technical_lines() {
        let state = RosterCardState::ReadyToSetUp;
        let mut fixture = input(&state);
        fixture.uid = None;
        fixture.fw = None;
        fixture.hardware = None;
        fixture.project_name = None;
        fixture.detected_chip = Some("esp32c6");
        fixture.port_label = Some("ESP32 Serial (0x303a:0x1001) · port-2");

        let view = device_rich_object(&fixture);
        let technical = view
            .sections
            .iter()
            .find(|section| section.title == "Technical")
            .expect("technical section exists pre-hello");
        let lines: Vec<(&str, &str)> = technical
            .lines
            .iter()
            .map(|line| (line.label.as_str(), line.value.as_str()))
            .collect();
        assert_eq!(
            lines,
            vec![
                ("chip", "esp32c6"),
                ("port", "ESP32 Serial (0x303a:0x1001) · port-2"),
                ("transport", "USB"),
            ]
        );
    }

    #[test]
    fn running_behind_live_device_sections_and_rollup() {
        let state = RosterCardState::RunningBehind {
            observed_version: Some(3),
            head_version: Some(5),
        };
        let view = device_rich_object(&input(&state));
        assert_eq!(
            titles(&view),
            vec!["Health", "Project", "Technical", "Danger zone"]
        );

        let rollup = view.rollup();
        assert_eq!(rollup.tone, UiStatusKind::Attention);
        // Health wins the Attention tie with Project (schema precedence), so
        // the primary affordance is the state table's push.
        assert_eq!(
            rollup.affordance,
            Some(&DeviceDetailAffordance::Roster(
                RosterAffordance::PushVersion { version: Some(5) }
            ))
        );

        let danger = view.sections.last().unwrap();
        assert_eq!(danger.weight, RichWeight::Danger);
        // A held project can always be wiped from here (2026-07-26 walk).
        assert_eq!(
            danger.affordances,
            vec![
                // Troubleshoot leads the danger zone in EVERY state
                // (2026-07-31) — recovery must not be gated on the ladder
                // having ended on Not-responding.
                DeviceDetailAffordance::Roster(RosterAffordance::Troubleshoot),
                // Backup is READ before the destructive verbs, because it is
                // what makes them survivable (M6).
                DeviceDetailAffordance::BackUpFilesystem,
                DeviceDetailAffordance::Roster(RosterAffordance::WipeProject),
                DeviceDetailAffordance::FlashFirmware,
                DeviceDetailAffordance::EraseDevice,
                DeviceDetailAffordance::DisconnectDevice,
            ]
        );
    }

    #[test]
    fn offline_device_gets_forget_and_a_neutral_rollup() {
        let state = RosterCardState::Offline {
            last_seen_at: Some(NOW - 2.0 * 86_400.0),
        };
        let mut input = input(&state);
        input.fw = None;
        let view = device_rich_object(&input);
        assert_eq!(
            titles(&view),
            vec!["Health", "Project", "Technical", "Danger zone"]
        );

        let rollup = view.rollup();
        assert_eq!(rollup.tone, UiStatusKind::Neutral);
        assert_eq!(
            rollup.affordance,
            Some(&DeviceDetailAffordance::Roster(RosterAffordance::Reconnect))
        );
        assert_eq!(
            view.sections.last().unwrap().affordances,
            vec![
                DeviceDetailAffordance::Roster(RosterAffordance::Troubleshoot),
                DeviceDetailAffordance::ForgetDevice
            ]
        );
    }

    #[test]
    fn diverged_device_carries_the_backup_section_in_schema_order() {
        let view = device_rich_object(&input(&RosterCardState::EditedOnDevice {
            local_saved_at: None,
            pushed_at: None,
        }));
        assert_eq!(
            titles(&view),
            vec!["Health", "Project", "Technical", "Backup", "Danger zone"]
        );
        // §3a: Health explains the situation in plain words (the Backup
        // section still carries the banked facts).
        let health = &view.sections[0];
        assert_eq!(health.lines.len(), 2);
        assert!(
            health.lines[1].value.contains("edits your project doesn't"),
            "the note explains, not just labels: {}",
            health.lines[1].value
        );
    }

    /// The state this milestone exists for: a board sitting in ROM download
    /// mode, whose own project is what stopped it. Backup must be reachable
    /// from here, and it must be READ before the verbs that destroy the
    /// thing it saves.
    #[test]
    fn a_board_in_recovery_mode_can_back_up_before_it_is_flashed_or_erased() {
        let mut input = input(&RosterCardState::RecoveryMode);
        input.project_name = None;
        input.fw = None;

        let view = device_rich_object(&input);

        assert_eq!(
            view.sections.last().unwrap().affordances,
            vec![
                DeviceDetailAffordance::Roster(RosterAffordance::Troubleshoot),
                DeviceDetailAffordance::BackUpFilesystem,
                DeviceDetailAffordance::FlashFirmware,
                DeviceDetailAffordance::EraseDevice,
                DeviceDetailAffordance::DisconnectDevice,
            ]
        );
    }

    /// A blank or foreign board has no `lpfs` to read — the row could only
    /// ever fail, so it is not offered.
    #[test]
    fn a_blank_or_foreign_board_is_offered_no_backup() {
        for state in [
            RosterCardState::ReadyToSetUp,
            RosterCardState::OtherFirmware,
        ] {
            let view = device_rich_object(&input(&state));
            assert!(
                !view
                    .sections
                    .last()
                    .unwrap()
                    .affordances
                    .contains(&DeviceDetailAffordance::BackUpFilesystem),
                "{state:?} has nothing to back up"
            );
        }
    }

    #[test]
    fn firmware_chip_is_advisory_and_never_colors_the_rollup() {
        let bundled = BundledFirmware {
            commit: "def987654321".to_string(),
            dirty: false,
        };
        let mut input = input(&RosterCardState::RunningUpToDate);
        input.bundled_fw = Some(&bundled);
        let view = device_rich_object(&input);

        let technical = view
            .sections
            .iter()
            .find(|section| section.title == "Technical")
            .unwrap();
        let chip = technical.chip.as_ref().expect("chip offered");
        assert_eq!(chip.text, "Firmware update available");
        assert_eq!(chip.tone, UiStatusKind::Attention);
        // …but the rollup stays the Health section's Good.
        assert_eq!(view.rollup().tone, UiStatusKind::Good);
    }

    /// Gaps-only: a device that can do everything says nothing extra, so
    /// the one line that matters on a lesser board is not buried.
    #[test]
    fn an_all_capable_device_adds_no_capability_lines() {
        let view = device_rich_object(&input(&RosterCardState::RunningUpToDate));
        let technical = view
            .sections
            .iter()
            .find(|section| section.title == "Technical")
            .unwrap();
        let labels: Vec<&str> = technical
            .lines
            .iter()
            .map(|line| line.label.as_str())
            .collect();
        assert_eq!(labels, vec!["uid", "transport", "firmware"]);
    }

    /// A build without the fluid/radio runtimes and a unit with no radio
    /// wired: each absence gets exactly one line, naming what is missing.
    #[test]
    fn a_device_with_gaps_names_each_one() {
        let gapped_build = BuildFacts {
            features: DEVICE_FW
                .features
                .iter()
                .copied()
                .filter(|feature| {
                    !matches!(
                        feature,
                        LpFeature::NodeFluid | LpFeature::NodeRadio | LpFeature::SvcRadioEspnow
                    )
                })
                .collect(),
            ..DEVICE_FW.clone()
        };
        let gapped_hw = HardwareFacts {
            radio: false,
            button: true,
            board_id: None,
            ..Default::default()
        };
        let state = RosterCardState::RunningUpToDate;
        let mut input = input(&state);
        input.fw = Some(&gapped_build);
        input.hardware = Some(&gapped_hw);

        let view = device_rich_object(&input);
        let technical = view
            .sections
            .iter()
            .find(|section| section.title == "Technical")
            .unwrap();
        let lines: Vec<(&str, &str)> = technical
            .lines
            .iter()
            .map(|line| (line.label.as_str(), line.value.as_str()))
            .collect();
        assert!(lines.contains(&("no nodes", "Fluid · Radio")), "{lines:?}");
        assert!(lines.contains(&("no hardware", "radio")), "{lines:?}");
        // The CPU shader backend is the norm and stays silent.
        assert!(
            !lines.iter().any(|(label, _)| *label == "graphics"),
            "{lines:?}"
        );
    }

    /// P5: the old-format card leads with the verb that fixes it, and the
    /// way out stays reachable underneath. The card carries no project
    /// chip (its content never resolved to a running library project), so
    /// the wipe row has to be offered on the STATE, not on the chip.
    #[test]
    fn an_upgradable_board_leads_with_upgrade_and_still_offers_the_way_out() {
        let state = RosterCardState::HoldsOldFormatProject {
            standing: crate::app::roster::DeviceFormatStanding::Upgradable { found: 4 },
            expected: lpc_model::PROJECT_FORMAT_VERSION,
        };
        let mut fixture = input(&state);
        fixture.project_name = None;
        let view = device_rich_object(&fixture);

        assert_eq!(
            view.rollup().affordance,
            Some(&DeviceDetailAffordance::Roster(
                RosterAffordance::UpgradeProject
            ))
        );
        assert!(
            view.sections
                .last()
                .unwrap()
                .affordances
                .contains(&DeviceDetailAffordance::Roster(
                    RosterAffordance::WipeProject
                )),
            "starting over must stay reachable: {:?}",
            view.sections.last().unwrap().affordances
        );
    }

    /// A format with no upgrade path already LEADS with wipe on Health —
    /// offering it again in the danger zone would be two buttons for one
    /// decision.
    #[test]
    fn a_board_with_no_upgrade_path_offers_the_wipe_exactly_once() {
        let state = RosterCardState::HoldsOldFormatProject {
            standing: crate::app::roster::DeviceFormatStanding::TooOld { found: Some(2) },
            expected: lpc_model::PROJECT_FORMAT_VERSION,
        };
        let mut fixture = input(&state);
        fixture.project_name = None;
        let view = device_rich_object(&fixture);

        let wipes = view
            .sections
            .iter()
            .flat_map(|section| section.affordances.iter())
            .filter(|affordance| {
                **affordance == DeviceDetailAffordance::Roster(RosterAffordance::WipeProject)
            })
            .count();
        assert_eq!(wipes, 1, "one offer, not two");
        assert_eq!(
            view.rollup().affordance,
            Some(&DeviceDetailAffordance::Roster(
                RosterAffordance::WipeProject
            ))
        );
    }

    #[test]
    fn working_states_carry_no_danger_zone_and_no_primary_affordance() {
        let state = RosterCardState::OperationInFlight {
            label: "Installing firmware".to_string(),
            percent: Some(62),
        };
        let view = device_rich_object(&input(&state));
        assert!(!titles(&view).contains(&"Danger zone"));
        assert_eq!(view.rollup().affordance, None);
        assert_eq!(view.rollup().tone, UiStatusKind::Attention);
    }

    const NOW: f64 = 1_800_000_000.0;

    fn input<'a>(state: &'a RosterCardState) -> DeviceRichInput<'a> {
        DeviceRichInput {
            state,
            uid: Some("dev_7pQr5St89uVwXy2C"),
            transport: "USB",
            project_name: Some("porch-sign"),
            fw: Some(&DEVICE_FW),
            hardware: Some(&DEVICE_HW),
            bundled_fw: None,
            detected_chip: None,
            port_label: None,
            now_secs: NOW,
        }
    }

    static DEVICE_FW: std::sync::LazyLock<BuildFacts> = std::sync::LazyLock::new(|| BuildFacts {
        features: vec![
            LpFeature::NodeButton,
            LpFeature::NodeClock,
            LpFeature::NodeFluid,
            LpFeature::NodeFixture,
            LpFeature::NodePlaylist,
            LpFeature::NodeRadio,
            LpFeature::NodeShader,
            LpFeature::NodeTexture,
            LpFeature::SvcButton,
            LpFeature::SvcRadioEspnow,
            LpFeature::GfxLpvm,
        ],
        package: "fw-esp32c6".to_string(),
        commit: "abc123456789".to_string(),
        dirty: false,
        profile: "release-esp32".to_string(),
    });

    /// An all-capable unit: the gaps-only Technical lines add nothing here,
    /// which is the point.
    static DEVICE_HW: std::sync::LazyLock<HardwareFacts> =
        std::sync::LazyLock::new(|| HardwareFacts {
            radio: true,
            button: true,
            board_id: None,
            ..Default::default()
        });

    fn titles(view: &RichObjectView<DeviceDetailAffordance>) -> Vec<&str> {
        view.sections
            .iter()
            .map(|section| section.title.as_str())
            .collect()
    }
}
