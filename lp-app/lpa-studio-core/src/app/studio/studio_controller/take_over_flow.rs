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
//!    card) connects, and each nameless port of the kind this tab could not
//!    open — kept shut, read as held, refused, or refused after the release
//!    — re-identifies, once ([`FreedPorts`]). Only the freed port can open,
//!    and its hello says which board it is, so no pairing of ports to
//!    boards is ever needed. A board held by its
//!    network slot is reached by the board's ordinary connect instead: its
//!    own network link here (one this tab let go reopens), else over Wi‑Fi
//!    at the address this browser remembers, else through lightplayer.app
//!    when someone is signed in ([`NetworkRoad`]); with none of them the
//!    offer says "No way to reach it from here".
//! 4. A busy holder's refusal, "not held" while another tab has it, no
//!    answer, a network connect that fails (in its own words), or a board
//!    that does not open in time end the take-over with the reason
//!    ([`crate::UiTakeOver`]).
//!
//! The offer's words and level are `take_over_offer`'s; WHEN it is offered
//! is decided here ([`StudioController::take_over_offer_for`]).

use std::rc::Rc;

use lpa_devices::{BoardKey, DeviceStatus, HoldVia};

use super::StudioController;
use crate::app::devices::board_hold::{
    AskOutcome, AskRefusal, FreedPorts, HoldKey, is_network_road, usb_pair_of,
};
use crate::app::devices::take_over_state::{
    ASK_PATIENCE_SECS, NETWORK_OPEN_PATIENCE_SECS, OPEN_PATIENCE_SECS, TAKE_OVER_ANOTHER_TAB,
    TAKE_OVER_NO_WAY, TakeOverTimeout,
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
        // Held by its USB port, the board opens on a port this tab has; held
        // by its network slot, it needs a road from here.
        let reachable = held.via == HoldVia::Usb || self.network_road_from_here(view.id).is_some();
        Some(crate::take_over_offer(
            &facts.prefix,
            view.id,
            &held.level,
            self.take_overs.asking(view.id),
            reachable,
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
        let (Some(edge), Some(book)) =
            (self.board_hold_edge.clone(), self.board_hold_book.as_mut())
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
        // Done: the board is ready here, or the roster no longer has it
        // (forgotten) and there is no card to say anything on.
        let done: Vec<crate::DeviceId> = self
            .take_overs
            .devices()
            .filter(|device| !self.take_overs.asking(*device))
            .filter(|device| match self.devices.roster().device(*device) {
                Some(device) => {
                    matches!(
                        device.status(),
                        DeviceStatus::Ready | DeviceStatus::Degraded
                    )
                }
                None => true,
            })
            .collect();
        for device in done {
            self.take_overs.done(device);
        }
    }

    /// A connect over Wi‑Fi or through lightplayer.app to the board with
    /// `mac` ended (`failure`: its words, when it failed). A take-over that
    /// ran it ends with them; a success is done once the board is ready.
    pub(super) fn take_over_reach_ended(&mut self, mac: BoardKey, failure: Option<String>) {
        let Some(failure) = failure else {
            return;
        };
        let opening: Vec<crate::DeviceId> = self
            .take_overs
            .devices()
            .filter(|device| {
                self.take_overs.is_opening(*device) && self.board_key(*device) == Some(mac)
            })
            .collect();
        for device in opening {
            self.take_overs.fail(device, failure.clone());
        }
    }

    /// The holder let go of `device`'s board (`mac`): open it here.
    ///
    /// For a USB hold, every port of its kind the hold kept shut leaves the
    /// gate; the board's own link connects when its card has one, and each
    /// nameless port of the kind this tab could not open re-identifies
    /// ([`StudioController::open_ports_take_overs_free`]), as does one
    /// whose refusal is heard after this. The OS lets only the freed one
    /// open. With no hold named (it came free before the ask), the board's
    /// own link connects, and the ports of its link's kind are freed the
    /// same way. A board held by its network slot is reached by its
    /// ordinary connect ([`Self::reach_taken_board_over_network`]).
    fn open_taken_board(&mut self, device: crate::DeviceId, mac: BoardKey, key: Option<HoldKey>) {
        let now = (self.now_secs)();
        // The road the hold was on: the key asked about, or (when it came
        // free before the ask) the fact the card wears.
        let via = key.map(|key| key.via()).or_else(|| {
            self.devices
                .roster()
                .device(device)?
                .evidence
                .held_elsewhere
                .as_ref()
                .map(|held| held.via)
        });
        if via == Some(HoldVia::Network) {
            self.take_overs
                .opening_within(device, now, NETWORK_OPEN_PATIENCE_SECS);
            self.wake_board_holds_after(NETWORK_OPEN_PATIENCE_SECS);
            self.journal_hold(format!("hold: reaching {mac} here over the network"));
            self.reach_taken_board_over_network(device);
            return;
        }
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
        if let Some(pair) = pair {
            let gate = Rc::clone(self.devices.effects().hold_gate());
            gate.borrow_mut().release_pair(pair);
            self.board_hold_flow
                .freeing
                .insert(device, FreedPorts::new(pair));
        }
        let has_link = self
            .devices
            .roster()
            .device(device)
            .is_some_and(|device| device.link().is_some());
        if has_link {
            self.fold_device_input(DeviceInput::Action(DeviceAction::Connect { device }));
        }
        self.open_ports_take_overs_free();
    }

    /// The board's ordinary connect, once its holder let go of its network
    /// slot: by the first road this tab has ([`NetworkRoad`]); with none,
    /// the take-over ends "No way to reach it from here".
    fn reach_taken_board_over_network(&mut self, device: crate::DeviceId) {
        let started = match self.network_road_from_here(device) {
            Some(NetworkRoad::Link) => {
                self.fold_device_input(crate::DeviceInput::Action(DeviceAction::Connect {
                    device,
                }));
                Ok(UiNotices::new())
            }
            Some(NetworkRoad::Wifi) => {
                self.start_wifi_connect(crate::WifiConnectOp::Board { device })
            }
            Some(NetworkRoad::Relay) => self.start_relay_connect(crate::RelayConnectOp { device }),
            None => Err(UiError::UnsupportedAction(TAKE_OVER_NO_WAY.to_string())),
        };
        if let Err(error) = started {
            let words = match error {
                UiError::UnsupportedAction(words) => words,
                other => other.to_string(),
            };
            self.take_overs.fail(device, words);
        }
    }

    /// How this tab would reach `device`'s board over its network slot,
    /// first road first; `None` when it has none.
    fn network_road_from_here(&self, device: crate::DeviceId) -> Option<NetworkRoad> {
        let roster = self.devices.roster();
        let entry = roster.device(device)?;
        if entry
            .link()
            .and_then(|link| roster.link_info(link))
            .is_some_and(is_network_road)
        {
            return Some(NetworkRoad::Link);
        }
        let mac = self.board_key(device)?;
        if self.lan_transport.is_some()
            && self
                .wifi_addresses
                .get(&mac)
                .and_then(crate::WifiAddress::url)
                .is_some()
        {
            return Some(NetworkRoad::Wifi);
        }
        if self.relay_transport.is_some() && self.access.account_keys().is_some() {
            return Some(NetworkRoad::Relay);
        }
        None
    }
}

/// The roads this tab has to a board's network slot, first first.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NetworkRoad {
    /// A network link the roster has for the board here (a session this
    /// tab closed by request when it let the board go): it reopens.
    Link,
    /// The address this browser remembers for the board on Wi‑Fi.
    Wifi,
    /// lightplayer.app's relay, with someone signed in.
    Relay,
}
