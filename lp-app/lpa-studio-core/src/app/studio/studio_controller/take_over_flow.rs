//! One tab holds a board, the asker's side: Connect on a board another tab
//! holds (`devices/<board>/take-over`).
//!
//! 1. A board the book no longer lists as held (it came free meanwhile) is
//!    simply opened.
//! 2. Otherwise the holder is asked on the hold channel (a USB hold before
//!    a network one), and has five seconds to answer.
//! 3. `Released` — or the holder's `Gone`, whichever comes first — opens
//!    the board here: every port of the hold's kind that the hold kept shut
//!    leaves the gate, the board's own link (when one was merged onto its
//!    card) connects, and each nameless held port re-identifies. Only the
//!    freed port can open, and its hello says which board it is, so no
//!    pairing of ports to boards is ever needed.
//! 4. A busy holder's refusal, "not held" while another tab has it, no
//!    answer, or a board that does not open in ten seconds end the
//!    take-over with the reason ([`crate::UiTakeOver`]).
//!
//! The offer's words and level are `take_over_offer`'s; WHEN it is offered
//! is decided here ([`StudioController::take_over_offer_for`]).

use std::collections::BTreeSet;
use std::rc::Rc;

use lpa_devices::link::LinkId;
use lpa_devices::{BoardKey, DeviceStatus, HoldVia};

use super::StudioController;
use crate::app::devices::board_hold::{AskOutcome, AskRefusal, HoldKey, usb_pair_of};
use crate::app::devices::take_over_state::{
    ASK_PATIENCE_SECS, OPEN_PATIENCE_SECS, TAKE_OVER_ANOTHER_TAB, TakeOverTimeout,
};
use crate::core::notice::UiNotices;
use crate::{DeviceAction, DeviceInput, UiError, UiResult};

impl StudioController {
    /// `devices/<board>/take-over`: Connect on a board another tab holds,
    /// when the board wears the fact and is not a runtime (a sim or an
    /// in-tab emulated board has no port to hold). Disabled while its
    /// holder is being asked; its level is the holder's last word.
    pub(super) fn take_over_offer_for(
        &self,
        view: &crate::DeviceView,
        facts: &crate::DeviceOfferFacts<'_>,
    ) -> Option<crate::UiOffer> {
        let held = view.held_elsewhere.as_ref()?;
        if self.board_hold_edge.is_none() || facts.face != crate::DeviceFace::Wire {
            return None;
        }
        Some(crate::take_over_offer(
            &facts.prefix,
            view.id,
            &held.level,
            self.take_overs.asking(view.id),
        ))
    }

    /// Run a press of `take-over`.
    pub(super) fn begin_take_over(&mut self, op: crate::TakeOverOp) -> UiResult {
        let device = op.device;
        let Some(mac) = self.board_key(device) else {
            return Err(UiError::UnsupportedAction(
                "this board has not said who it is yet".to_string(),
            ));
        };
        let (Some(edge), Some(book)) = (self.board_hold_edge.clone(), self.board_hold_book.as_mut())
        else {
            return Err(UiError::UnsupportedAction(
                "this browser cannot ask its other tabs".to_string(),
            ));
        };
        if self.take_overs.asking(device) {
            return Ok(UiNotices::new());
        }
        // A USB hold before a network one: the port is what this tab can
        // open the moment it is let go.
        let key = book
            .others_for_mac(mac)
            .map(|(key, _)| *key)
            .min_by_key(|key| key.via() != HoldVia::Usb);
        let now = (self.now_secs)();
        match key {
            // It came free since the card was drawn: just open it.
            None => self.open_taken_board(device, mac, None),
            Some(key) => {
                let request = self.take_overs.mint_request();
                let note = book.ask(request, key);
                edge.post(&note);
                self.take_overs.ask(device, request, key, now);
                self.wake_board_holds_after(ASK_PATIENCE_SECS);
                self.journal_hold(format!("hold: asked the other tab for {key}"));
            }
        }
        self.mark_dirty();
        Ok(UiNotices::new())
    }

    /// The holder answered this tab's ask number `request`.
    pub(super) fn take_over_answered(&mut self, request: u64, key: HoldKey, outcome: AskOutcome) {
        let Some(device) = self.take_overs.device_asking(request) else {
            return;
        };
        match outcome {
            AskOutcome::Released => self.open_taken_board(device, key.mac(), Some(key)),
            AskOutcome::Refused(AskRefusal::Busy(label)) => {
                self.take_overs
                    .fail(device, crate::busy_in_the_other_tab(&label));
            }
            AskOutcome::Refused(AskRefusal::NotHeld) => {
                let held = self
                    .board_hold_book
                    .as_ref()
                    .is_some_and(|book| book.others_for_mac(key.mac()).next().is_some());
                match held {
                    // Another asker came first: it is theirs now (Retry asks
                    // them).
                    true => self.take_overs.fail(device, TAKE_OVER_ANOTHER_TAB),
                    false => self.open_taken_board(device, key.mac(), Some(key)),
                }
            }
        }
        self.mark_dirty();
    }

    /// Another tab's hold on `key` ended (its `Gone`, or the sentinel): an
    /// ask waiting on it opens the board now, whichever came first.
    pub(super) fn take_over_freed(&mut self, key: HoldKey) {
        for (device, request) in self.take_overs.asking_for(&key) {
            if let Some(book) = self.board_hold_book.as_mut() {
                book.abandon_ask(request);
            }
            self.open_taken_board(device, key.mac(), Some(key));
        }
    }

    /// Deadlines and endings: an unanswered ask fails, an open that never
    /// came fails, and a take-over whose board is ready here is done.
    pub(super) fn reconcile_take_overs(&mut self, now: f64) {
        for timeout in self.take_overs.expire(now) {
            match timeout {
                TakeOverTimeout::NoAnswer { request, .. } => {
                    if let Some(book) = self.board_hold_book.as_mut() {
                        book.abandon_ask(request);
                    }
                    self.journal_hold("hold: the other tab did not answer".to_string());
                }
                TakeOverTimeout::StillInUse { .. } => {
                    self.journal_hold("hold: the freed board did not open here".to_string());
                }
            }
        }
        let ready: Vec<crate::DeviceId> = self
            .take_overs
            .devices()
            .filter(|device| !self.take_overs.asking(*device))
            .filter(|device| {
                self.devices.roster().device(*device).is_some_and(|device| {
                    matches!(device.status(), DeviceStatus::Ready | DeviceStatus::Degraded)
                })
            })
            .collect();
        for device in ready {
            self.take_overs.done(device);
        }
    }

    /// The holder let go of `device`'s board (`mac`): open it here.
    ///
    /// For a USB hold, every port of its kind the hold kept shut leaves the
    /// gate; the board's own link connects when its card has one, and each
    /// nameless held port re-identifies. The OS lets only the freed one
    /// open. With no hold named (it came free before the ask), the board's
    /// own link connects, and the ports of its link's kind are freed the
    /// same way.
    fn open_taken_board(&mut self, device: crate::DeviceId, mac: BoardKey, key: Option<HoldKey>) {
        let now = (self.now_secs)();
        self.take_overs.opening(device, now);
        self.wake_board_holds_after(OPEN_PATIENCE_SECS);
        self.journal_hold(format!("hold: opening {mac} here"));
        let pair = key.and_then(|key| key.usb_pair()).or_else(|| {
            let roster = self.devices.roster();
            roster
                .device(device)?
                .link()
                .and_then(|link| roster.link_info(link))
                .and_then(usb_pair_of)
        });
        let mut ports: BTreeSet<LinkId> = BTreeSet::new();
        if let Some(pair) = pair {
            let gate = Rc::clone(self.devices.effects().hold_gate());
            ports.extend(gate.borrow_mut().release_pair(pair));
            ports.extend(
                self.devices
                    .roster()
                    .pending()
                    .iter()
                    .filter(|pending| {
                        pending.evidence().link_held_by_tab()
                            && usb_pair_of(&pending.info) == Some(pair)
                    })
                    .map(|pending| pending.link),
            );
        }
        let has_link = self
            .devices
            .roster()
            .device(device)
            .is_some_and(|device| device.link().is_some());
        if has_link {
            self.fold_device_input(DeviceInput::Action(DeviceAction::Connect { device }));
        }
        let nameless: Vec<crate::DeviceId> = self
            .devices
            .roster()
            .pending()
            .iter()
            .filter(|pending| ports.contains(&pending.link))
            .map(|pending| pending.device_id())
            .collect();
        for pending in nameless {
            self.fold_device_input(DeviceInput::Action(DeviceAction::Identify {
                device: pending,
            }));
        }
    }
}
