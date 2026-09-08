//! The two hops between the model's projection and a rendered card: which
//! status tone a state wears, and which action an escape dispatches.
//!
//! Both live in core rather than in the renderer for the usual reason — they
//! are decisions, and decisions get tests. The escape mapping in particular is
//! load-bearing: invariant I3 says every card carries at least one escape, and
//! a renderer that silently dropped one would defeat the invariant from
//! outside the model, where no property test can see it.

use lpa_devices::Action;
use lpa_devices::device::DeviceStatus;
use lpa_devices::identity::DeviceId;
use lpa_devices::link::LinkId;
use lpa_devices::view::Escape;

use crate::{UiAction, UiStatusKind};

use super::devices_op::DevicesOp;

/// The tone a device's headline state wears.
pub fn device_status_kind(status: DeviceStatus) -> UiStatusKind {
    match status {
        DeviceStatus::Ready => UiStatusKind::Good,
        DeviceStatus::Busy => UiStatusKind::Working,
        // Not an error: a board that is plugged in and has not been asked
        // anything is fine, and one that is unplugged is not a fault.
        DeviceStatus::Attached | DeviceStatus::Offline => UiStatusKind::Neutral,
        // A running board with a faulted node or a non-green recovery
        // state: the same tone as a board that needs firmware, because it
        // wants the same thing — a person to look at it.
        DeviceStatus::Degraded | DeviceStatus::NeedsAttention => UiStatusKind::Attention,
        DeviceStatus::NotResponding => UiStatusKind::Warning,
    }
}

/// The action an escape on a device card dispatches.
pub fn device_escape_action(escape: Escape, device: DeviceId) -> UiAction {
    device_escape_action_for(escape, device, super::DeviceFace::Wire)
}

/// The same, for a device whose FACE decides the words on two of the verbs
/// (PD8/Q15): a sim is powered on and off, a board is connected and
/// disconnected. Identical dispatch — the action is the same `Connect` /
/// `Disconnect` either way, because a sim's link is a link — and identical
/// everywhere else, because a Forget takes the same things away.
pub fn device_escape_action_for(
    escape: Escape,
    device: DeviceId,
    face: super::DeviceFace,
) -> UiAction {
    let action = match (escape, face) {
        (Escape::Cancel, _) => Action::CancelActivity { device },
        (Escape::Retry, _) => Action::Identify { device },
        // Reconnect asks the browser's chooser for a port back, which is
        // meaningless for a runtime this tab makes: the remembered line's
        // Reconnect slot is where a powered-off sim's **Power on** lives
        // (Q5), and Power on is `Connect` (PD8).
        (Escape::Reconnect, super::DeviceFace::Sim) => Action::Connect { device },
        (Escape::Reconnect, super::DeviceFace::Wire) => Action::Reconnect { device },
        (Escape::Disconnect, _) => Action::Disconnect { device },
        (Escape::Forget, _) => Action::Forget { device },
    };
    match face {
        super::DeviceFace::Sim => DevicesOp::sim_action_for(action),
        super::DeviceFace::Wire => DevicesOp::action_for(action),
    }
}

/// The action an escape on a PENDING LINK dispatches.
///
/// A pending link is not a device, so its escapes address the link: the
/// projection expresses dismissal as [`Escape::Forget`], and dismissing hands
/// the grant back. `Cancel` stops the identification that is running on it.
pub fn pending_escape_action(escape: Escape, link: LinkId) -> UiAction {
    DevicesOp::action_for(match escape {
        Escape::Cancel
        | Escape::Retry
        | Escape::Reconnect
        | Escape::Disconnect
        | Escape::Forget => Action::DismissLink { link },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_status_wears_a_tone_and_only_trouble_reads_as_trouble() {
        assert_eq!(device_status_kind(DeviceStatus::Ready), UiStatusKind::Good);
        assert_eq!(
            device_status_kind(DeviceStatus::Busy),
            UiStatusKind::Working
        );
        assert_eq!(
            device_status_kind(DeviceStatus::Offline),
            UiStatusKind::Neutral,
            "an unplugged board is not a fault"
        );
        assert_eq!(
            device_status_kind(DeviceStatus::NeedsAttention),
            UiStatusKind::Attention
        );
        assert_eq!(
            device_status_kind(DeviceStatus::NotResponding),
            UiStatusKind::Warning
        );
        assert_eq!(
            device_status_kind(DeviceStatus::Degraded),
            UiStatusKind::Attention,
            "a degraded board must not wear the Ready tone"
        );
        assert_ne!(
            device_status_kind(DeviceStatus::Degraded),
            device_status_kind(DeviceStatus::Ready),
            "the whole point of Degraded is that it does not read as Ready"
        );
    }

    /// I3 from the renderer's side: every escape the projection can produce
    /// has an action behind it, so a card can never show a way out that does
    /// nothing.
    #[test]
    fn every_escape_dispatches_something() {
        let device = DeviceId(1);
        for escape in [Escape::Cancel, Escape::Disconnect, Escape::Forget] {
            let action = device_escape_action(escape, device);
            let op = action
                .op_as::<DevicesOp>()
                .expect("an escape is a device action");
            assert_eq!(op.action.device(), Some(device), "{escape:?}");
        }
    }

    #[test]
    fn a_pending_links_escapes_all_address_the_link() {
        let link = LinkId(4);
        for escape in [Escape::Cancel, Escape::Disconnect, Escape::Forget] {
            let action = pending_escape_action(escape, link);
            let op = action.op_as::<DevicesOp>().expect("a device action");
            assert_eq!(op.action, Action::DismissLink { link }, "{escape:?}");
        }
    }
}
