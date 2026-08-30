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
    pub activity: Option<ActivityView>,
    /// Survives disconnect; cleared when a new activity supersedes it.
    pub last_outcome: Option<OutcomeView>,
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
    pub title: String,
    pub state_label: String,
    pub detail: Option<String>,
    /// "Set up this device" — always offered, because a blank chip may never
    /// identify itself.
    pub can_adopt: bool,
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
        activity,
        last_outcome: device.evidence.last_outcome.as_ref().map(outcome_view),
        escapes,
    }
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
        title: if entry.info.label.is_empty() {
            "New device".to_string()
        } else {
            entry.info.label.clone()
        },
        state_label,
        detail,
        can_adopt: true,
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
        DeviceStatus::Attached => "Attached — port closed".to_string(),
        DeviceStatus::Ready => "Ready".to_string(),
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
