//! The holder's side of an `Ask`: refuse, or let the board go — in an
//! order that leaves the asker a free port.
//!
//! A holder that is working on the board (`Busy`) refuses with the
//! activity's own label, and nothing changes. A holder asked for a board it
//! does not hold (any more — another asker came first) answers `NotHeld`.
//! Otherwise it lets go, in this order:
//!
//! 1. the editor lens on that board closes;
//! 2. the board's last picture goes to its sidecar now, past the ten-second
//!    limit, so the tab that takes it over shows the newest frame;
//! 3. the link is disconnected (intent Disconnected: no sweep, retry or
//!    hotplug reopens it);
//! 4. the port's close is awaited;
//! 5. the lock is released;
//! 6. `Released` is said to the asker, then `Gone` to everyone, and this
//!    tab's board wears "taken by another tab".
//!
//! Steps 1 and 2 to 3 run in the controller (the frame write is async);
//! [`PendingRelease`] is what it keeps between them.
//!
//! The whole release is one budget, [`RELEASE_PATIENCE_SECS`], counted from
//! the ask: the lock goes by then whether or not the port has closed. The
//! asker's patience (`take_over_state::ASK_PATIENCE_SECS`) is this budget
//! with room to spare, never only its first step: a fallback counted from
//! after the picture write could outlast it on a slow machine
//! (`docs/defects/2026-10-09-a-holders-release-can-outlast-the-askers-five-seconds.md`).

use lpa_devices::{DeviceId, HoldLevel};

use super::hold_key::HoldKey;
use super::hold_note::AskRefusal;
use super::tab_id::TabId;

/// The most a holder takes from hearing an ask to letting the lock go: the
/// picture, the disconnect and the port's close fit inside it, and when the
/// port has not closed by then the lock goes anyway.
pub const RELEASE_PATIENCE_SECS: f64 = 3.0;

/// What the holder does with one `Ask`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AnswerPlan {
    /// Say no, and why.
    Refuse(AskRefusal),
    /// Let the board go (the steps above).
    Release,
}

/// The answer to an ask for a board this tab holds at `level` (`None`: does
/// not hold it). `releasing`: this tab is already letting that board go for
/// an earlier ask, which wins.
pub fn answer_plan(level: Option<&HoldLevel>, releasing: bool) -> AnswerPlan {
    match level {
        None => AnswerPlan::Refuse(AskRefusal::NotHeld),
        Some(_) if releasing => AnswerPlan::Refuse(AskRefusal::NotHeld),
        Some(HoldLevel::Busy(label)) => AnswerPlan::Refuse(AskRefusal::Busy(label.clone())),
        Some(HoldLevel::Watching | HoldLevel::Open) => AnswerPlan::Release,
    }
}

/// A board being let go in answer to an ask.
#[derive(Clone, Debug, PartialEq)]
pub struct PendingRelease {
    pub request: u64,
    pub asker: TabId,
    pub key: HoldKey,
    /// The roster device whose port this is, when this tab still has it.
    pub device: Option<DeviceId>,
    pub stage: ReleaseStage,
    /// When the lock goes even if the port has not closed (epoch seconds):
    /// [`RELEASE_PATIENCE_SECS`] after the ask was heard.
    pub deadline: f64,
}

/// Where a release stands.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReleaseStage {
    /// The lens is closed; the final picture and the disconnect are next.
    WriteFrame,
    /// Disconnected; waiting for the port to close.
    WaitClose,
}

impl PendingRelease {
    /// Letting go of `key` (on `device`) for `asker`'s ask number `request`,
    /// heard at `now`.
    pub fn new(
        request: u64,
        asker: TabId,
        key: HoldKey,
        device: Option<DeviceId>,
        now: f64,
    ) -> Self {
        Self {
            request,
            asker,
            key,
            device,
            stage: ReleaseStage::WriteFrame,
            deadline: now + RELEASE_PATIENCE_SECS,
        }
    }

    /// Whether the lock may go now: disconnected, and the port has closed
    /// (`closed`) or the release's budget ran out.
    pub fn ready_to_release(&self, closed: bool, now: f64) -> bool {
        match self.stage {
            ReleaseStage::WriteFrame => false,
            ReleaseStage::WaitClose => closed || now >= self.deadline,
        }
    }
}

#[cfg(test)]
mod tests {
    use lpa_devices::BoardKey;

    use super::*;
    use crate::app::devices::board_hold::hold_key::UsbPair;

    #[test]
    fn a_busy_holder_refuses_with_its_label_and_an_idle_one_lets_go() {
        assert_eq!(
            answer_plan(Some(&HoldLevel::Busy("Flashing \u{b7} 40%".into())), false),
            AnswerPlan::Refuse(AskRefusal::Busy("Flashing \u{b7} 40%".into()))
        );
        assert_eq!(
            answer_plan(Some(&HoldLevel::Watching), false),
            AnswerPlan::Release
        );
        assert_eq!(
            answer_plan(Some(&HoldLevel::Open), false),
            AnswerPlan::Release
        );
    }

    #[test]
    fn a_board_not_held_or_already_going_is_not_held() {
        assert_eq!(
            answer_plan(None, false),
            AnswerPlan::Refuse(AskRefusal::NotHeld)
        );
        assert_eq!(
            answer_plan(Some(&HoldLevel::Watching), true),
            AnswerPlan::Refuse(AskRefusal::NotHeld),
            "the first asker wins"
        );
    }

    #[test]
    fn the_lock_goes_once_the_port_closed_or_the_wait_ran_out() {
        let mut release = release_heard_at(7.0);
        assert!(
            !release.ready_to_release(true, 7.0),
            "the frame comes first"
        );

        release.stage = ReleaseStage::WaitClose;
        assert!(!release.ready_to_release(false, 9.0));
        assert!(release.ready_to_release(true, 9.0));
        assert!(release.ready_to_release(false, 7.0 + RELEASE_PATIENCE_SECS));
    }

    /// The budget runs from the ask, not from the disconnect: a picture
    /// write that took most of it leaves the close only what remains.
    #[test]
    fn a_slow_picture_write_does_not_stretch_the_release() {
        let mut release = release_heard_at(100.0);
        // The write took 2.5 s; the port is now closing.
        release.stage = ReleaseStage::WaitClose;
        assert!(!release.ready_to_release(false, 102.5));
        assert!(
            release.ready_to_release(false, 100.0 + RELEASE_PATIENCE_SECS),
            "not {RELEASE_PATIENCE_SECS} s after the write"
        );
    }

    fn release_heard_at(now: f64) -> PendingRelease {
        PendingRelease::new(
            1,
            TabId::new("b"),
            HoldKey::usb(
                BoardKey::parse("a0:f2:62:87:b4:8c").expect("a mac"),
                UsbPair {
                    vendor: 0x303a,
                    product: 0x1001,
                },
            ),
            Some(DeviceId(3)),
            now,
        )
    }
}
