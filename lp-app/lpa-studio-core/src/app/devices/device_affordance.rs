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
        DeviceStatus::NeedsAttention => UiStatusKind::Attention,
        DeviceStatus::NotResponding => UiStatusKind::Warning,
    }
}

/// The action an escape on a device card dispatches.
pub fn device_escape_action(escape: Escape, device: DeviceId) -> UiAction {
    DevicesOp::action_for(match escape {
        Escape::Cancel => Action::CancelActivity { device },
        Escape::Retry => Action::Identify { device },
        Escape::Reconnect => Action::Reconnect { device },
        Escape::Disconnect => Action::Disconnect { device },
        Escape::Forget => Action::Forget { device },
    })
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
            assert_eq!(op.0.device(), Some(device), "{escape:?}");
        }
    }

    #[test]
    fn a_pending_links_escapes_all_address_the_link() {
        let link = LinkId(4);
        for escape in [Escape::Cancel, Escape::Disconnect, Escape::Forget] {
            let action = pending_escape_action(escape, link);
            let op = action.op_as::<DevicesOp>().expect("a device action");
            assert_eq!(op.0, Action::DismissLink { link }, "{escape:?}");
        }
    }
}
