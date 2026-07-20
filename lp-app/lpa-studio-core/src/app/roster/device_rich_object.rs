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
//! | Project | drift (Warning on behind/diverged) | Actionable | a project is known (held now or last ran) |
//! | Technical | Neutral (+ advisory fw chip) | Advisory | uid / transport / hello fw evidence exists |
//! | Performance | Neutral/Warning | Advisory | never yet — `ProjectRuntimeSummary` does not flow to cards |
//! | Backup | Neutral | Actionable | a device copy was banked at connect (diverged) |
//! | Danger zone | Neutral (never colors rollup) | Danger | live manageable link → flash+erase; offline registered → forget |
//!
//! The Health section IS today's card derivation: its tone and affordance
//! come straight from [`RosterCardState`] (itself the product of
//! [`derive_roster_card_state`](super::derive_roster_card_state)), so the
//! popover can never disagree with the circle.

use lpc_wire::FwProvenance;

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
    /// Danger zone, live device: install/repair firmware.
    FlashFirmware,
    /// Danger zone, live device: wipe the flash (confirmed).
    EraseDevice,
    /// Danger zone, offline registered device: forget it (D34 hygiene).
    ForgetDevice,
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
    /// Running-firmware provenance from the hello (live links only).
    pub fw: Option<&'a FwProvenance>,
    /// Studio's bundled firmware image, when the packaged manifest is on
    /// hand — the advisory chip comparison's other half.
    pub bundled_fw: Option<&'a BundledFirmware>,
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
    // The sub-line rides along as a fact row — except the diverged banked
    // note, which is the Backup section's whole story.
    if !matches!(input.state, RosterCardState::EditedOnDevice)
        && let Some(sub_line) = input.state.sub_line()
    {
        lines.push(RichLine::new("note", sub_line));
    }
    RichSection {
        title: "Health".to_string(),
        tone: input.state.circle().tone,
        lines,
        chip: None,
        affordances: input
            .state
            .affordance()
            .map(DeviceDetailAffordance::Roster)
            .into_iter()
            .collect(),
        weight: RichWeight::Actionable,
    }
}

/// Project: what the device holds (live) or last ran (offline), with the
/// drift facts. Drift tone matches the card grammar (Warning on
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
            tone = UiStatusKind::Warning;
            lines.push(RichLine::new(
                "running",
                match observed_version {
                    Some(version) => format!("{name} · v{version}"),
                    None => name.to_string(),
                },
            ));
            if let Some(head) = head_version {
                lines.push(RichLine::new("your copy", format!("v{head}")));
            }
            affordances.push(DeviceDetailAffordance::Roster(RosterAffordance::OpenEditor));
        }
        RosterCardState::EditedOnDevice => {
            tone = UiStatusKind::Warning;
            lines.push(RichLine::new(
                "running",
                format!("{name} · edited on device"),
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
    if lines.is_empty() {
        return None;
    }
    let chip = input
        .bundled_fw
        .zip(input.fw)
        .filter(|(bundled, fw)| bundled.update_available(fw))
        .map(|_| RichChip {
            tone: UiStatusKind::Warning,
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
fn backup_section(input: &DeviceRichInput<'_>) -> Option<RichSection<DeviceDetailAffordance>> {
    matches!(input.state, RosterCardState::EditedOnDevice).then(|| RichSection {
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

/// Danger zone, pinned last. Live cards whose wire could take management
/// operations carry flash + erase (the same gate the interim More-menu
/// used — never mid-operation, mid-connect, or while another tab holds
/// the port); offline registered cards carry forget. M8's provisioning
/// entries get their permanent home here.
fn danger_section(input: &DeviceRichInput<'_>) -> Option<RichSection<DeviceDetailAffordance>> {
    let affordances = match input.state {
        RosterCardState::Offline { .. } => {
            if input.uid.is_some() {
                vec![DeviceDetailAffordance::ForgetDevice]
            } else {
                Vec::new()
            }
        }
        RosterCardState::ConnectingRetrying { .. }
        | RosterCardState::OperationInFlight { .. }
        | RosterCardState::InUseElsewhere => Vec::new(),
        _ => vec![
            DeviceDetailAffordance::FlashFirmware,
            DeviceDetailAffordance::EraseDevice,
        ],
    };
    if affordances.is_empty() {
        return None;
    }
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
        assert_eq!(rollup.tone, UiStatusKind::Warning);
        // Health wins the Warning tie with Project (schema precedence), so
        // the primary affordance is the state table's push.
        assert_eq!(
            rollup.affordance,
            Some(&DeviceDetailAffordance::Roster(
                RosterAffordance::PushVersion { version: Some(5) }
            ))
        );

        let danger = view.sections.last().unwrap();
        assert_eq!(danger.weight, RichWeight::Danger);
        assert_eq!(
            danger.affordances,
            vec![
                DeviceDetailAffordance::FlashFirmware,
                DeviceDetailAffordance::EraseDevice,
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
            vec![DeviceDetailAffordance::ForgetDevice]
        );
    }

    #[test]
    fn diverged_device_carries_the_backup_section_in_schema_order() {
        let view = device_rich_object(&input(&RosterCardState::EditedOnDevice));
        assert_eq!(
            titles(&view),
            vec!["Health", "Project", "Technical", "Backup", "Danger zone"]
        );
        // the banked note lives in Backup, not duplicated into Health
        let health = &view.sections[0];
        assert_eq!(health.lines.len(), 1);
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
        assert_eq!(chip.tone, UiStatusKind::Warning);
        // …but the rollup stays the Health section's Good.
        assert_eq!(view.rollup().tone, UiStatusKind::Good);
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
        assert_eq!(view.rollup().tone, UiStatusKind::Warning);
    }

    const NOW: f64 = 1_800_000_000.0;

    fn input<'a>(state: &'a RosterCardState) -> DeviceRichInput<'a> {
        DeviceRichInput {
            state,
            uid: Some("dev_7pQr5St89uVwXy2C"),
            transport: "USB",
            project_name: Some("porch-sign"),
            fw: Some(&DEVICE_FW),
            bundled_fw: None,
            now_secs: NOW,
        }
    }

    static DEVICE_FW: std::sync::LazyLock<FwProvenance> =
        std::sync::LazyLock::new(|| FwProvenance {
            package: "fw-esp32".to_string(),
            commit: "abc123456789".to_string(),
            dirty: false,
            profile: "release-esp32".to_string(),
        });

    fn titles(view: &RichObjectView<DeviceDetailAffordance>) -> Vec<&str> {
        view.sections
            .iter()
            .map(|section| section.title.as_str())
            .collect()
    }
}
