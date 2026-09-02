//! The projection: view DTOs that are pure functions of (intent, evidence,
//! activity).
//!
//! Two properties are enforced by property tests rather than by
//! hand-audited match arms (see `tests/properties.rs`):
//!
//! 1. **Total** — every reachable model state renders something honest. No
//!    "unreachable" arm, no empty card.
//! 2. **Escapable** — every [`DeviceView`] carries at least one
//!    [`Escape`], and pending links carry dismiss. The shipped system lost
//!    its danger zone in exactly the stuck states; here Forget is defined at
//!    the model level, so it cannot be conditioned away.
//!
//! Nothing in this file stores anything. Rendering a device twice yields the
//! same DTO.

use serde::{Deserialize, Serialize};

use crate::activity::{ActivityKind, ActivityOutcome, CancelPhase};
use crate::device::{Device, DeviceStatus};
use crate::evidence::{Classification, IncompatibleReason, Liveness};
use crate::identity::DeviceId;
use crate::link::LinkId;
use crate::roster::{PendingLink, Roster};
use crate::time::{Millis, describe_age_ms};
use crate::wire::{ProjectFaultFacts, RecoveryFacts, RecoveryLevelFacts, RecoveryPathFacts};

/// Everything the device surface needs to draw itself.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RosterView {
    pub devices: Vec<DeviceView>,
    /// Roster-level "new device found, identifying…" entries.
    pub pending: Vec<PendingLinkView>,
}

/// One device card.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DeviceView {
    pub id: DeviceId,
    pub title: String,
    pub status: DeviceStatus,
    /// Headline line ("Ready", "Blank flash — needs firmware", "Offline").
    pub state_label: String,
    /// Second line: what the evidence actually says.
    pub detail: Option<String>,
    /// Honest staleness ("last heard 12 s ago") instead of a stuck spinner.
    pub freshness_label: Option<String>,
    /// Strongest identity binding, for disambiguating two similar boards.
    pub identity_label: Option<String>,
    /// Chip read off the boot banner, normalized ("esp32c6"). What the
    /// needs-firmware face filters the board pick by.
    pub detected_chip: Option<String>,
    /// The board this firmware says it was built for, from the hello's
    /// hardware facts (`/hardware.json`, written at flash and effective the
    /// NEXT boot). `None` until the board reports one — the empty face's
    /// "a new project" entry is honestly disabled rather than guessing a
    /// pin map.
    pub board_id: Option<String>,
    /// The fold says this board wants firmware (blank, bootloader, foreign,
    /// or incompatible) — the face that offers a board pick + Flash.
    pub needs_firmware: bool,
    /// ONE line naming why a running board is not running well: a node in
    /// fault, or a recovery state the device reported as not green.
    ///
    /// It rides UNDER the running face rather than replacing it — a
    /// degraded board is still running, and a card that dropped the project
    /// name would answer "what is on it?" with a complaint. `None` = the
    /// board reported nothing wrong, which on an embedder that reports no
    /// recovery at all means only "no fault", never "healthy".
    pub degraded: Option<String>,
    /// What the board says it is running. The empty face and the running
    /// face are the two answers; [`LoadedProject::Unknown`] is the third,
    /// and it offers neither rather than guessing.
    pub loaded_project: LoadedProject,
    /// Whether a project could be sent to this board right now: a
    /// proto-compatible LightPlayer, on a link, with nothing else running.
    /// The empty face's primary verb is drawn only when this is true.
    pub can_receive_project: bool,
    /// Whether "Remove project" is a real verb right now: the board has
    /// REPORTED something loaded, the port is open, and nothing else is
    /// running. Never offered on [`LoadedProject::Unknown`] — a board that
    /// has not said what it holds must not be offered a delete.
    pub can_remove_project: bool,
    pub activity: Option<ActivityView>,
    /// Survives disconnect; cleared when a new activity supersedes it.
    pub last_outcome: Option<OutcomeView>,
    /// The card's terminal panel: what the board said and what Studio did
    /// to it, oldest first. Serial lines and activity narration interleaved,
    /// because that is the order they happened in — a flash's progress used
    /// to be visible only in the browser console (G1 bench, 2026-08-31).
    pub terminal_lines: Vec<String>,
    /// Never empty (invariant I3).
    pub escapes: Vec<Escape>,
}

/// The running activity, as the card shows it.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ActivityView {
    pub kind: ActivityKind,
    pub label: String,
    pub percent: Option<u8>,
    pub cancellable: bool,
    /// A cancel has been asked for and the activity is winding down.
    pub cancel_requested: bool,
}

/// What a board is running, as the card is allowed to state it.
///
/// Three answers, not two. Collapsing [`Self::Unknown`] into [`Self::Empty`]
/// would put "Put something on it" on a board whose project we simply have
/// not been told about yet — the over-claim the whole model leans away from.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum LoadedProject {
    /// The board has not reported yet (no heartbeat carrying the fact since
    /// the port opened, or firmware too old to send it).
    #[default]
    Unknown,
    /// The board reported its loaded list, and it was empty.
    Empty,
    /// The board is running something, named by the storage dir it runs
    /// from — the only name the wire carries.
    Running { label: String },
}

/// A finished activity's outcome line.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct OutcomeView {
    pub summary: String,
    pub ok: bool,
}

/// A way out. Every card has at least one.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum Escape {
    Cancel,
    /// Re-grant the port through the chooser — offered on an offline card
    /// that is WORTH reconnecting (known identity), because some bridges'
    /// grants cannot survive a replug.
    Reconnect,
    /// Re-run identification — offered when the link is up but the last
    /// identify settled in silence, so "try again" needs no replug.
    Retry,
    Disconnect,
    Forget,
}

/// A link the roster is still identifying.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PendingLinkView {
    pub link: LinkId,
    /// The provisional device id, minted at discovery — what a Flash gesture
    /// on this entry targets (the gesture adopts, and adoption keeps the id).
    pub device: DeviceId,
    pub title: String,
    pub state_label: String,
    pub detail: Option<String>,
    /// "Set up this device" — always offered, because a blank chip may never
    /// identify itself.
    pub can_adopt: bool,
    /// The settled verdict says this board wants firmware — the pending
    /// entry's needs-firmware face (board pick + Flash, which adopts).
    pub needs_firmware: bool,
    /// Chip read off the boot banner, normalized ("esp32c6").
    pub detected_chip: Option<String>,
    /// Dismiss, expressed as [`Escape::Forget`].
    pub escapes: Vec<Escape>,
}

/// Project the whole roster.
pub fn roster_view(roster: &Roster, now: Millis) -> RosterView {
    RosterView {
        devices: roster
            .devices()
            .iter()
            .map(|device| device_view(device, now))
            .collect(),
        pending: roster
            .pending()
            .iter()
            .map(|entry| pending_link_view(entry, now))
            .collect(),
    }
}

/// Project one device.
///
/// `now` is a parameter rather than a clock read: the freshness label is the
/// whole point of the projection ("last heard 12 s ago"), and this crate
/// reads no clocks.
pub fn device_view(device: &Device, now: Millis) -> DeviceView {
    let status = device.status();
    let loaded = loaded_project(device);
    let activity = device.activity.as_ref().map(|cell| ActivityView {
        kind: cell.kind,
        label: format!("{}…", cell.kind.label()),
        percent: cell.progress.as_ref().and_then(|progress| progress.percent),
        cancellable: !cell.is_cancel_requested(),
        cancel_requested: matches!(cell.cancel, CancelPhase::CancelRequested { .. }),
    });

    let mut escapes = Vec::new();
    if let Some(view) = &activity {
        if view.cancellable {
            escapes.push(Escape::Cancel);
        }
    }
    // The one verdict identify can END in is silence, and the escape from
    // silence must not be a replug: offer the re-ask whenever the link is
    // up, nothing is running, and the evidence still says nothing.
    if activity.is_none()
        && device.link().is_some()
        && matches!(
            device.evidence.classification,
            Classification::Quiet { .. } | Classification::Unknown
        )
    {
        escapes.push(Escape::Retry);
    }
    if device.link().is_some() {
        escapes.push(Escape::Disconnect);
    } else if !device.identity.is_anonymous() {
        escapes.push(Escape::Reconnect);
    }
    // Defined at the model level, therefore never conditioned away.
    escapes.push(Escape::Forget);

    DeviceView {
        id: device.id,
        title: device.title(),
        status,
        state_label: state_label(device, status),
        detail: detail(device),
        freshness_label: freshness_label(device, now),
        identity_label: device.identity.strongest_label(),
        detected_chip: device.evidence.detected_chip().map(str::to_string),
        board_id: device
            .evidence
            .classification
            .hello()
            .and_then(|hello| hello.board_id.clone()),
        needs_firmware: needs_firmware(&device.evidence.classification),
        degraded: degraded(device),
        // The mirror of the push condition, over the OTHER answer: a board
        // that reported something running, on an open port, idle.
        can_remove_project: matches!(loaded, LoadedProject::Running { .. })
            && device.evidence.presence.is_open()
            && device.activity.is_none(),
        loaded_project: loaded,
        // An OPEN port, not merely an attached one: the push conversation
        // talks over this port, and a verb that could only fail is worse
        // than no verb. (The fold already implies it — closing a port
        // begins a fresh window, which drops the LightPlayer verdict — but
        // stating it keeps the two from drifting apart.)
        can_receive_project: device.evidence.classification.is_light_player()
            && device.evidence.presence.is_open()
            && device.activity.is_none(),
        activity,
        last_outcome: device.evidence.last_outcome.as_ref().map(outcome_view),
        terminal_lines: device
            .evidence
            .recent_output()
            .map(str::to_string)
            .collect(),
        escapes,
    }
}

/// Whether a classification is one Flash fixes: blank or erased flash, a
/// board parked in the ROM downloader, somebody else's firmware, a
/// LightPlayer this build cannot speak to — or a board that answers
/// NOTHING. The settled-quiet arm is the old wizard's Unresponsive
/// verdict, which offered flash after its bootloader escalation; our
/// parking IS that escalation now, and without this arm a silent chip
/// (parked in the downloader by a failed flash, say) had no verb at all
/// (it bit three times at G1, 2026-08-31). The widened pick's copy says
/// the pick is checked against the silicon before anything is written —
/// the chip guard is the safety, here as everywhere.
fn needs_firmware(classification: &Classification) -> bool {
    matches!(
        classification,
        Classification::Blank
            | Classification::Bootloader
            | Classification::Foreign { .. }
            | Classification::Incompatible { .. }
            | Classification::Quiet { .. }
    )
}

/// What this board is running, from its own report.
///
/// Only a LightPlayer gets an answer: a blank chip's silence is not an empty
/// project storage, and offering to push at a board that cannot receive one
/// would be a verb that does nothing.
fn loaded_project(device: &Device) -> LoadedProject {
    if !device.evidence.classification.is_light_player() {
        return LoadedProject::Unknown;
    }
    match device.evidence.loaded_projects() {
        None => LoadedProject::Unknown,
        Some([]) => LoadedProject::Empty,
        // Firmware loads one project; a host server with several has no card
        // to draw, so the first entry is the answer.
        Some([first, ..]) => LoadedProject::Running {
            label: first.label().to_string(),
        },
    }
}

/// The ONE line that says why a running board is not running well.
///
/// Deterministic by construction — same evidence, same words, in the same
/// order — because it sits on a card that redraws every heartbeat, and a
/// line that reshuffled its clauses would read as new news every second.
///
/// Only a LightPlayer on an OPEN port gets one: degradation is a live
/// report, and repeating the last thing a board said before we stopped
/// listening would age into a lie.
fn degraded(device: &Device) -> Option<String> {
    let evidence = &device.evidence;
    if !evidence.classification.is_light_player() || !evidence.presence.is_open() {
        return None;
    }
    let recovery = evidence
        .recovery()
        .filter(|recovery| recovery.is_degraded());
    // The fault leads when there is one: it names the node, which is the
    // actionable half. Recovery is the device-wide backdrop, and a fault
    // raised BY the ledger already quotes it in its own message.
    let line = evidence
        .project_fault()
        .and_then(fault_line)
        .or_else(|| recovery.map(recovery_line))?;
    Some(line)
}

/// "Degraded: node /x faulted — <why>", plus a count when several are.
fn fault_line(fault: &ProjectFaultFacts) -> Option<String> {
    let (path, message) = fault.nodes.first()?;
    let mut line = format!("Degraded: node {path} faulted — {message}");
    let others = fault.nodes.len().saturating_sub(1);
    if others > 0 {
        line.push_str(&format!(" (+{others} more)"));
    }
    Some(line)
}

/// The device-wide recovery state in one sentence.
fn recovery_line(recovery: &RecoveryFacts) -> String {
    let gated: Vec<&RecoveryPathFacts> = recovery.gated().collect();
    let mut line = match recovery.level {
        RecoveryLevelFacts::Red => match gated.as_slice() {
            [] => "Recovery red: a path is disabled after repeated crashes".to_string(),
            [only] => format!(
                "Recovery red: {} disabled after repeated crashes",
                only.label
            ),
            [first, ..] => format!(
                "Recovery red: {} paths disabled (first: {})",
                gated.len(),
                first.label
            ),
        },
        RecoveryLevelFacts::Yellow => match recovery.paths.len() {
            1 => "Recovery yellow: 1 watched path".to_string(),
            watched => format!("Recovery yellow: {watched} watched paths"),
        },
        // Green and still here means safe mode is the whole story.
        RecoveryLevelFacts::Green => {
            "Safe mode: project auto-load skipped after repeated incomplete boots".to_string()
        }
    };
    if let Some(crash) = &recovery.last_crash {
        line.push_str(&format!(" (last crash: {crash})"));
    }
    if recovery.safe_mode && recovery.level != RecoveryLevelFacts::Green {
        line.push_str(" · safe mode");
    }
    line
}

/// Project one pending link.
pub fn pending_link_view(entry: &PendingLink, now: Millis) -> PendingLinkView {
    let state_label = match entry.verdict() {
        None => "New device found — identifying…".to_string(),
        Some(classification) => format!(
            "New device found — {}",
            classification_label(classification)
        ),
    };
    let detail = entry
        .evidence()
        .detected_chip()
        .map(|chip| format!("chip: {chip}"))
        .or_else(|| {
            entry
                .evidence()
                .recent_lines()
                .last()
                .map(|line| line.to_string())
        })
        .or_else(|| Some(format!("found {}", describe_age_ms(now.since(entry.since)))));

    PendingLinkView {
        link: entry.link,
        device: entry.device_id(),
        title: if entry.info.label.is_empty() {
            "New device".to_string()
        } else {
            entry.info.label.clone()
        },
        state_label,
        detail,
        can_adopt: true,
        // Only a SETTLED verdict offers the flash face: mid-identification
        // the honest face is "identifying…", not a premature verb.
        needs_firmware: entry.verdict().is_some_and(needs_firmware),
        detected_chip: entry.evidence().detected_chip().map(str::to_string),
        escapes: vec![Escape::Forget],
    }
}

fn state_label(device: &Device, status: DeviceStatus) -> String {
    match status {
        DeviceStatus::Busy => device
            .activity_kind()
            .map(|kind| format!("{}…", kind.label()))
            .unwrap_or_else(|| "Working…".to_string()),
        DeviceStatus::Offline => "Offline".to_string(),
        // "port closed" was true and useless: it named an implementation
        // detail for a board that is plugged in and simply not being
        // listened to. On the bench it read as a fault (G1, 2026-08-31).
        DeviceStatus::Attached => "Attached — not listening".to_string(),
        DeviceStatus::Ready => "Ready".to_string(),
        DeviceStatus::Degraded => "Degraded".to_string(),
        DeviceStatus::NotResponding => "Not responding".to_string(),
        DeviceStatus::NeedsAttention => classification_label(&device.evidence.classification),
    }
}

fn classification_label(classification: &Classification) -> String {
    match classification {
        Classification::Unknown => "Identifying…".to_string(),
        Classification::LightPlayer { hello } => format!("LightPlayer · {}", hello.label()),
        Classification::Incompatible {
            reason: IncompatibleReason::ProtoMismatch { proto },
        } => format!("Incompatible firmware (wire proto {proto})"),
        Classification::Incompatible {
            reason: IncompatibleReason::NoHello,
        } => "No LightPlayer hello — pre-hello firmware".to_string(),
        Classification::Blank => "Blank flash — needs firmware".to_string(),
        Classification::Bootloader => "Waiting in ROM download mode".to_string(),
        Classification::Foreign { label: Some(label) } => format!("Running {label}"),
        Classification::Foreign { label: None } => "Unrecognized firmware".to_string(),
        Classification::Quiet { .. } => "Not responding".to_string(),
    }
}

fn detail(device: &Device) -> Option<String> {
    if let Some(hello) = device.evidence.classification.hello() {
        return Some(hello.label());
    }
    if let Some(chip) = device.evidence.detected_chip() {
        return Some(format!("chip: {chip}"));
    }
    device
        .evidence
        .recent_lines()
        .last()
        .map(|line| line.to_string())
}

fn freshness_label(device: &Device, now: Millis) -> Option<String> {
    let freshness = &device.evidence.freshness;
    let age = freshness.age_ms(now)?;
    Some(match freshness.state {
        Liveness::Quiet => format!("quiet — last heard {}", describe_age_ms(age)),
        Liveness::Live | Liveness::Unknown => format!("last heard {}", describe_age_ms(age)),
    })
}

fn outcome_view(outcome: &ActivityOutcome) -> OutcomeView {
    OutcomeView {
        summary: outcome.summary(),
        ok: outcome.is_success(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::IdentityChain;

    #[test]
    fn every_card_offers_forget_even_with_nothing_known() {
        let device = Device::new(DeviceId(1), IdentityChain::default());

        let view = device_view(&device, Millis(0));

        assert_eq!(view.escapes, vec![Escape::Forget]);
        assert_eq!(view.state_label, "Offline");
        assert_eq!(view.title, "New device");
    }

    /// V3/CH340 (G1 2026-08-31): a bridge with no USB serial number loses
    /// its grant on replug and the page CANNOT see the board again — the
    /// known-but-offline card must offer the way back itself.
    #[test]
    fn a_known_offline_device_offers_reconnect() {
        let device = Device::new(
            DeviceId(3),
            IdentityChain {
                uid: Some(crate::identity::DeviceUid("dev_abc".to_string())),
                ..Default::default()
            },
        );

        let view = device_view(&device, Millis(0));

        assert_eq!(view.escapes, vec![Escape::Reconnect, Escape::Forget]);
    }

    #[test]
    fn an_unnamed_offline_device_still_renders_something_honest() {
        let mut device = Device::new(DeviceId(2), IdentityChain::default());
        device.evidence.last_outcome = Some(ActivityOutcome::Failed {
            message: "flash failed".to_string(),
        });

        let view = device_view(&device, Millis(1_000));

        assert_eq!(
            view.last_outcome,
            Some(OutcomeView {
                summary: "flash failed".to_string(),
                ok: false
            }),
            "outcomes survive disconnect"
        );
    }

    #[test]
    fn classification_labels_never_fall_through_to_a_placeholder() {
        let labels = [
            Classification::Unknown,
            Classification::Blank,
            Classification::Bootloader,
            Classification::Foreign { label: None },
            Classification::Foreign {
                label: Some("WLED".to_string()),
            },
            Classification::Incompatible {
                reason: IncompatibleReason::NoHello,
            },
            Classification::Incompatible {
                reason: IncompatibleReason::ProtoMismatch { proto: 3 },
            },
            Classification::Quiet { since: Millis(0) },
        ];

        for classification in labels {
            let label = classification_label(&classification);
            assert!(!label.is_empty(), "{classification:?} rendered nothing");
        }
    }
}
