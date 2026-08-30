//! The two input streams and the command vocabulary.
//!
//! One entry point, two arms with different rights (the ratified ruling):
//!
//! - [`Action`] — a user gesture. It may write [`Intent`](crate::Intent),
//!   spawn or cancel an activity, and emit commands. It may **never** write
//!   [`Evidence`](crate::Evidence).
//! - [`Event`] — something the world did. It is folded into evidence and
//!   forwarded to the running activity. It may **never** write intent.
//!
//! Anything the model needs done leaves as a [`Command`]. The model never
//! performs IO, so this list is the complete set of side effects the device
//! layer can cause.

use serde::{Deserialize, Serialize};

use crate::activity::{ActivityKind, ActivityOutcome};
use crate::identity::DeviceId;
use crate::link::{LinkCommand, LinkEvent, LinkId, LinkInfo};
use crate::record::DeviceRecord;
use crate::time::TimerId;

/// One thing that happened, from either stream.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum Input {
    Action(Action),
    Event(Event),
}

impl Input {
    pub fn action(action: Action) -> Self {
        Self::Action(action)
    }

    pub fn event(event: Event) -> Self {
        Self::Event(event)
    }

    /// Convenience for the common `Event::Link { .. }` shape.
    pub fn link(link: LinkId, event: LinkEvent) -> Self {
        Self::Event(Event::Link { link, event })
    }
}

/// A user gesture. Prescriptive: it says what the user wants, not what is.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum Action {
    /// "Add a device" — ask the platform for a USB grant.
    AddFromUsb,
    /// "Set up this device" on a still-unidentified pending link. A blank
    /// chip may never identify itself, so user action must be able to create
    /// a device entry on its own.
    AdoptLink {
        link: LinkId,
    },
    /// Dismiss a pending link: close it and give the grant back.
    DismissLink {
        link: LinkId,
    },
    Connect {
        device: DeviceId,
    },
    Disconnect {
        device: DeviceId,
    },
    /// Delete the entry, its record, and its grant. Defined at the model
    /// level, therefore reachable from EVERY state — including mid-activity
    /// and including an anonymous board.
    Forget {
        device: DeviceId,
    },
    CancelActivity {
        device: DeviceId,
    },
    /// Re-run identification (the escape from a stale verdict).
    Identify {
        device: DeviceId,
    },
    SetName {
        device: DeviceId,
        name: String,
    },
    SetAutoconnect {
        device: DeviceId,
        enabled: bool,
    },
}

impl Action {
    /// Which device this gesture targets, when it targets one.
    pub fn device(&self) -> Option<DeviceId> {
        match self {
            Self::Connect { device }
            | Self::Disconnect { device }
            | Self::Forget { device }
            | Self::CancelActivity { device }
            | Self::Identify { device }
            | Self::SetName { device, .. }
            | Self::SetAutoconnect { device, .. } => Some(*device),
            Self::AddFromUsb | Self::AdoptLink { .. } | Self::DismissLink { .. } => None,
        }
    }
}

/// Something the world did.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum Event {
    /// A transport spoke. Routed to a device by the link map, or to the
    /// pending link that owns it.
    Link {
        link: LinkId,
        event: LinkEvent,
    },
    /// A transport appeared (plug, or a grant that resolved).
    LinkAttached {
        link: LinkId,
        info: LinkInfo,
    },
    /// A transport vanished (unplug, or the platform revoked it).
    LinkDetached {
        link: LinkId,
    },
    TimerFired {
        timer: TimerId,
    },
    /// A marker from a coarse effect the model asked for but does not drive
    /// frame by frame (esptool-js flashing, in round 2). Activity brackets
    /// the model raises itself are journaled directly.
    ActivityMarker {
        device: DeviceId,
        marker: ActivityMarker,
    },
}

/// The bracket-and-progress vocabulary of an activity's lifetime. The device
/// fold consumes these, which is how "busy with X" participates in derived
/// state without a parallel store (the `device_card_ops` disease).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ActivityMarker {
    Started {
        kind: ActivityKind,
    },
    /// Integer percent on purpose: journals stay byte-reproducible and
    /// fixtures can assert on progress.
    Progress {
        label: String,
        percent: Option<u8>,
    },
    Ended {
        kind: ActivityKind,
        outcome: ActivityOutcome,
    },
}

/// Something the effects layer must do. The model emits these and forgets
/// them; nothing here returns a value.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum Command {
    Link {
        link: LinkId,
        command: LinkCommand,
    },
    /// Come back with [`Event::TimerFired`] after `after_ms`. This is the
    /// only way the model waits.
    StartTimer {
        timer: TimerId,
        after_ms: u64,
    },
    /// Ask the platform for a USB device grant (the picker).
    RequestUsbGrant,
    PersistRecord(DeviceRecord),
    DeleteRecord(DeviceId),
    /// Hand a grant back so the port stops being ours.
    RevokeGrant(LinkInfo),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_device_targeted_action_names_its_device() {
        let device = DeviceId(7);
        let targeted = [
            Action::Connect { device },
            Action::Disconnect { device },
            Action::Forget { device },
            Action::CancelActivity { device },
            Action::Identify { device },
            Action::SetName {
                device,
                name: "n".to_string(),
            },
            Action::SetAutoconnect {
                device,
                enabled: true,
            },
        ];

        for action in targeted {
            assert_eq!(action.device(), Some(device), "{action:?}");
        }

        assert_eq!(Action::AddFromUsb.device(), None);
        assert_eq!(
            Action::AdoptLink {
                link: crate::LinkId(1)
            }
            .device(),
            None
        );
    }

    #[test]
    fn inputs_round_trip_through_json_so_fixtures_can_hold_them() {
        let input = Input::link(
            crate::LinkId(3),
            LinkEvent::Line("ESP-ROM:esp32c6".to_string()),
        );

        let json = serde_json::to_string(&input).expect("serialize");
        let back: Input = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(back, input);
    }
}
