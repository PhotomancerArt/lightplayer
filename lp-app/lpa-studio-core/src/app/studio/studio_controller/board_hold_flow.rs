//! One tab holds a board, the holder's side: the controller's half of the
//! hold flows.
//!
//! The pure halves live in `app/devices/board_hold/` (the book, the plan,
//! the gate, the answer); this file reads the roster, folds the facts and
//! drives the edge. Everything here is idle without a hold edge
//! ([`StudioController::set_board_hold_edge`]), and every device flow is
//! then as it was before holds existed.
//!
//! - **Priming.** The first sweep waits for one look at the lock manager:
//!   what other tabs hold goes into the book before any port is opened.
//! - **Claims.** A board whose link is open here and whose hello named its
//!   MAC is claimed (open, hello, then lock, then announce) and announced
//!   at its level — its USB port, or its one network slot (the LAN or the
//!   relay); a board whose link closed is let go (the link is closed
//!   already, then the lock, then the announcement).
//! - **The gate.** The claims of other tabs keep the effects layer's gate
//!   current, and a port whose open the OS refused is read against them.
//! - **The answer.** An ask is refused while busy; otherwise the lens
//!   closes, the last picture is written, the link disconnects, its close
//!   is awaited, the lock goes, and `Released` and `Gone` are said.
//! - **The sentinel.** One watch per hold elsewhere: when it fires the
//!   holder let go or died, the fact clears — and nothing opens the port.
//! - **The yield.** A board's network slot goes to the newest client that
//!   proves the holder's key (every tab of one browser presents the same
//!   keys), and the client it leaves would redial and take it back. A tab
//!   that hears another tab say it holds a board whose network slot this tab
//!   had closes its own session BY REQUEST (no redial), and its board wears
//!   "taken by another tab". Connect is then the person's.
//! - **The facts.** Every board another tab holds wears the fact
//!   (`Event::BoardHeld`), folded only when it changed.
//!
//! Async answers come back as `StudioCommand::HoldEdge` on the actor's
//! queue ([`StudioController::on_hold_edge_event`]); time is the injected
//! clock, and the deadlines are woken by the device layer's timer.

use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use lpa_devices::activity::{ActivityKind, ActivityOutcome};
use lpa_devices::link::{LinkId, LinkInfo};
use lpa_devices::{BoardKey, ConnectionIntent, HoldLevel, HoldVia};

use super::StudioController;
use crate::app::devices::board_hold::{
    AnswerPlan, AskOutcome, BookChange, ClaimAnswer, HoldCandidate, HoldEdgeEvent, HoldKey,
    HoldNote, HoldPriming, PRIMING_PATIENCE_SECS, PendingRelease, RELEASE_CLOSE_PATIENCE_SECS,
    ReleaseStage, TabId, UsbPair, answer_plan, desired_facts, desired_holds, fact_changes,
    is_network_road, plan_holds, reads_as_held, usb_pair_of,
};
use crate::app::devices::device_transport::DeviceTransport;
use crate::core::log::DeviceEventKind;
use crate::{DeviceAction, DeviceEvent, DeviceInput, StudioCommand};

/// The device journal's scope for hold transitions (the `?record=`
/// timelines read them beside the model's own lines).
const HOLD_JOURNAL_SCOPE: &str = "board-hold";

impl StudioController {
    /// What this tab's hold edge answered, or a hold deadline that may
    /// have passed (`StudioCommand::HoldEdge`).
    pub fn on_hold_edge_event(&mut self, event: HoldEdgeEvent) {
        if self.board_hold_edge.is_none() {
            return;
        }
        match event {
            HoldEdgeEvent::HeldNow(keys) => self.finish_hold_priming(keys),
            HoldEdgeEvent::Claimed { key, answer } => self.hold_claim_answered(key, answer),
            HoldEdgeEvent::WatchEnded { key, watch, freed } => {
                if self.board_hold_flow.watching.get(&key) != Some(&watch) {
                    // A sentinel this tab has since replaced or stopped.
                    return;
                }
                self.board_hold_flow.watching.remove(&key);
                if freed {
                    // A hold read off the lock manager at load whose holder
                    // never said a word is a page that is gone: the old page
                    // of a reload, whose lock outlived it for a moment. Its
                    // freeing opens the ports it kept shut, as a fresh load
                    // would have. A holder that was heard from and then died
                    // opens nothing (R3).
                    let unheard = self
                        .board_hold_book
                        .as_ref()
                        .and_then(|book| book.held_elsewhere(&key))
                        .is_some_and(|hold| hold.tab.is_none());
                    let freed = self
                        .board_hold_book
                        .as_mut()
                        .and_then(|book| book.freed_by_watch(&key));
                    if freed.is_some() {
                        self.journal_hold(format!(
                            "hold: {key} came free (its tab let go or closed)"
                        ));
                        self.hold_freed(key, unheard);
                    }
                }
            }
            HoldEdgeEvent::Due => {}
        }
        self.reconcile_board_holds();
        self.run_due_device_sweep();
        self.mark_dirty();
    }

    /// React to what a note (or a priming, or a sentinel) changed in the
    /// book: answer an ask, hear an answer, re-announce for a new tab, let
    /// the ports of a freed kind out of the gate.
    pub(super) fn react_to_hold_changes(&mut self, changes: &[BookChange]) {
        for change in changes {
            match change {
                BookChange::HeldElsewhere {
                    key,
                    level,
                    new_holder,
                    ..
                } => {
                    self.journal_hold(format!(
                        "hold: {key} is held by another tab ({})",
                        level
                            .as_ref()
                            .map_or("level not said yet".to_string(), |level| format!(
                                "{level:?}"
                            ))
                    ));
                    if *new_holder && key.via() == HoldVia::Network {
                        self.yield_network_board(*key);
                    }
                }
                BookChange::Freed { key } => self.hold_freed(*key, false),
                BookChange::AskReceived {
                    from,
                    request,
                    key,
                    level,
                } => self.answer_hold_ask(from.clone(), *request, *key, level.clone()),
                BookChange::AnswerReceived {
                    request,
                    key,
                    outcome,
                } => self.take_over_answered(*request, *key, outcome.clone()),
                BookChange::WhoAsked { .. } => {
                    let (Some(edge), Some(book)) =
                        (self.board_hold_edge.as_ref(), self.board_hold_book.as_ref())
                    else {
                        continue;
                    };
                    for note in book.announcements() {
                        edge.post(&note);
                    }
                }
            }
        }
    }

    /// Whether a granted-port sweep may register ports now: always without
    /// a hold edge; with one, once the first look at the lock manager has
    /// answered (or been waited on long enough). Starts that look.
    pub(super) fn hold_priming_lets_sweep_run(&mut self) -> bool {
        if self.board_hold_edge.is_none() {
            return true;
        }
        self.start_hold_priming();
        let now = (self.now_secs)();
        if self.board_hold_flow.priming.overdue(now) {
            self.journal_hold("hold: the lock manager did not answer; sweeping anyway".into());
            self.board_hold_flow.priming = HoldPriming::Done;
        }
        self.board_hold_flow.priming.lets_sweep_run()
    }

    /// Reconcile every hold against the roster: the diff that claims,
    /// releases, re-announces, keeps the sentinels and the gate current,
    /// reads refused ports, and puts the facts on the boards. Idempotent;
    /// runs after every batch's folds and after every note.
    pub(crate) fn reconcile_board_holds(&mut self) {
        let Some(edge) = self.board_hold_edge.clone() else {
            return;
        };
        if self.board_hold_book.is_none() {
            return;
        }
        self.start_hold_priming();
        let now = (self.now_secs)();
        self.finish_due_hold_releases(now);

        // This tab's own holds — not the ones it gave up to another tab
        // while their links close.
        let mut desired = self.desired_board_holds();
        self.board_hold_flow
            .yielded
            .retain(|key| desired.contains_key(key));
        desired.retain(|key, _| !self.board_hold_flow.yielded.contains(key));
        let answering: BTreeSet<HoldKey> = self.board_hold_flow.releases.keys().copied().collect();
        let plan = {
            let book = self.board_hold_book.as_ref().expect("checked above");
            let mine: BTreeMap<HoldKey, HoldLevel> = book
                .mine()
                .map(|(key, level)| (*key, level.clone()))
                .collect();
            plan_holds(
                &desired,
                &mine,
                &self.board_hold_flow.claims_in_flight(),
                &self.board_hold_flow.unguarded,
                &answering,
            )
        };
        for key in plan.release {
            self.let_go_of_hold(key);
        }
        for (key, level) in plan.levels {
            if let Some(book) = self.board_hold_book.as_mut()
                && book.hold(key, level.clone())
                && let Some(note) = book.announcement(&key)
            {
                edge.post(&note);
                self.journal_hold(format!("hold: {key} now {level:?}"));
            }
        }
        for key in plan.claim {
            self.claim_board_hold(key);
        }
        // Holds announced while another tab still had the lock: claim
        // again, one claim at a time, until the lock is this tab's.
        let unlocked: Vec<HoldKey> = self
            .board_hold_book
            .as_ref()
            .map(|book| {
                book.unlocked()
                    .filter(|key| {
                        !self.board_hold_flow.claiming.contains_key(key)
                            && !self.board_hold_flow.unguarded.contains(key)
                    })
                    .copied()
                    .collect()
            })
            .unwrap_or_default();
        for key in unlocked {
            self.claim_board_hold(key);
        }
        // Where each board held over the network came from, for the yield.
        for key in desired.keys().filter(|key| key.via() == HoldVia::Network) {
            if let Some(info) = self.network_link_of(key.mac()) {
                self.board_hold_flow.network_roads.insert(key.mac(), info);
            }
        }
        // A board this tab holds again is not "taken by another tab".
        let held_macs: BTreeSet<BoardKey> = self
            .board_hold_book
            .as_ref()
            .map(|book| book.mine().map(|(key, _)| key.mac()).collect())
            .unwrap_or_default();
        self.board_hold_flow
            .taken_from_here
            .retain(|mac, _| !held_macs.contains(mac));

        self.sync_hold_sentinels();
        self.sync_hold_gate_claims();
        self.read_refused_ports();
        self.name_held_ports();
        self.put_hold_facts_on_boards();
        self.reconcile_take_overs(now);
    }

    /// The boards being let go on request whose picture is next: write it,
    /// then disconnect the link (intent Disconnected: nothing reopens it),
    /// and wait for the port to close. Async because the write is.
    pub(crate) async fn run_due_hold_releases(&mut self) {
        let due: Vec<(HoldKey, Option<crate::DeviceId>)> = self
            .board_hold_flow
            .releases
            .values()
            .filter(|release| release.stage == ReleaseStage::WriteFrame)
            .map(|release| (release.key, release.device))
            .collect();
        if due.is_empty() {
            return;
        }
        for (key, device) in due {
            if let Some(device) = device {
                self.persist_device_frame_now(device).await;
                self.fold_device_input(DeviceInput::Action(DeviceAction::Disconnect { device }));
                self.journal_hold(format!(
                    "hold: {key} last picture written and port closing, for another tab"
                ));
            }
            let deadline = (self.now_secs)() + RELEASE_CLOSE_PATIENCE_SECS;
            if let Some(release) = self.board_hold_flow.releases.get_mut(&key) {
                release.stage = ReleaseStage::WaitClose { deadline };
            }
            self.wake_board_holds_after(RELEASE_CLOSE_PATIENCE_SECS);
        }
    }

    /// Hide the pending links another tab's claims account for: when every
    /// claim of a held port's kind names a board this browser remembers and
    /// that board wears the fact, the port IS one of those boards, not a
    /// new device. One that is not accounted for stays, saying "open in
    /// another Studio tab".
    pub(super) fn hide_accounted_held_links(&self, view: &mut crate::DeviceRosterView) {
        let Some(book) = self.board_hold_book.as_ref() else {
            return;
        };
        let roster = self.devices.roster();
        let accounted = |pair: UsbPair| {
            let claims: Vec<BoardKey> = book
                .others()
                .filter(|(key, _)| key.usb_pair() == Some(pair))
                .map(|(key, _)| key.mac())
                .collect();
            !claims.is_empty()
                && claims.iter().all(|mac| {
                    roster.devices().iter().any(|device| {
                        device.record.is_some()
                            && device.evidence.held_elsewhere.is_some()
                            && device.identity.mac.as_ref().and_then(BoardKey::from_mac)
                                == Some(*mac)
                    })
                })
        };
        view.roster.pending.retain(|pending| {
            if !pending.held_by_tab {
                return true;
            }
            match roster.link_info(pending.link).and_then(usb_pair_of) {
                Some(pair) => !accounted(pair),
                None => true,
            }
        });
    }

    /// Start the first look at the lock manager, once the spawner and the
    /// queue are installed.
    fn start_hold_priming(&mut self) {
        if self.board_hold_flow.priming != HoldPriming::NotStarted {
            return;
        }
        let (Some(edge), Some(spawner), Some(tx)) = (
            self.board_hold_edge.clone(),
            self.wifi_spawner.clone(),
            self.wifi_tx.clone(),
        ) else {
            return;
        };
        self.board_hold_flow.priming = HoldPriming::Waiting {
            since: (self.now_secs)(),
        };
        let look = edge.held_now();
        spawner(Box::pin(async move {
            let keys = look.await;
            tx.send(StudioCommand::HoldEdge(HoldEdgeEvent::HeldNow(keys)));
        }));
        self.wake_board_holds_after(PRIMING_PATIENCE_SECS);
    }

    /// The first look answered: what other tabs hold goes into the book (at
    /// no level yet, read as the cautious `Open`), the holders are asked to
    /// say their levels, and the waiting sweep may run.
    fn finish_hold_priming(&mut self, keys: Vec<HoldKey>) {
        let Some(book) = self.board_hold_book.as_mut() else {
            return;
        };
        let changes = book.prime(keys);
        if let Some(edge) = self.board_hold_edge.as_ref() {
            edge.post(&HoldNote::Who);
        }
        self.board_hold_flow.priming = HoldPriming::Done;
        self.journal_hold(format!(
            "hold: {} board hold(s) by other tabs at load",
            changes.len()
        ));
        self.react_to_hold_changes(&changes);
    }

    /// The holds this tab should have now: every roster board whose link
    /// (its USB port, or its network slot) is open here and whose MAC is
    /// known, at its level.
    fn desired_board_holds(&self) -> BTreeMap<HoldKey, HoldLevel> {
        let now = self.device_now();
        let lens = self
            .pool
            .attached_session()
            .map(|session| session.attachment().device);
        let roster = self.devices.roster();
        desired_holds(roster.devices().iter().map(|device| {
            let view = lpa_devices::view::device_view(device, now);
            let mac = device.identity.mac.as_ref().and_then(BoardKey::from_mac);
            HoldCandidate {
                key: mac
                    .zip(device.link().and_then(|link| roster.link_info(link)))
                    .and_then(|(mac, info)| HoldKey::of_link(mac, info)),
                open: device.evidence.presence.is_open(),
                busy: view
                    .activity
                    .filter(|activity| activity.kind != ActivityKind::Identify)
                    .map(|activity| activity.label),
                lens: lens == Some(device.id),
            }
        }))
    }

    /// Claim `key`'s lock; the answer comes back as
    /// [`HoldEdgeEvent::Claimed`].
    fn claim_board_hold(&mut self, key: HoldKey) {
        let (Some(edge), Some(spawner), Some(tx)) = (
            self.board_hold_edge.clone(),
            self.wifi_spawner.clone(),
            self.wifi_tx.clone(),
        ) else {
            return;
        };
        self.board_hold_flow.claiming.insert(key, true);
        let claim = edge.claim(&key);
        spawner(Box::pin(async move {
            let answer = claim.await;
            tx.send(StudioCommand::HoldEdge(HoldEdgeEvent::Claimed {
                key,
                answer,
            }));
        }));
    }

    /// A claim answered: hold it and say so — unless the board was let go
    /// meanwhile, when the lock goes again.
    fn hold_claim_answered(&mut self, key: HoldKey, answer: ClaimAnswer) {
        let Some(edge) = self.board_hold_edge.clone() else {
            return;
        };
        let wanted = self.board_hold_flow.claiming.remove(&key).unwrap_or(false);
        let level = match wanted {
            true => self.desired_board_holds().get(&key).cloned(),
            false => None,
        };
        let Some(level) = level else {
            // Let go while the claim was in flight: whatever it took, it
            // goes again (the next reconcile claims afresh if the board is
            // wanted after all).
            if answer == ClaimAnswer::Held {
                edge.release(&key);
            }
            return;
        };
        let Some(book) = self.board_hold_book.as_mut() else {
            return;
        };
        // A claim again, for a hold announced while another tab still had
        // its lock.
        let announced = book.holds(&key).is_some();
        match (answer, announced) {
            // The lock is ours now: say so, and other tabs watch it.
            (ClaimAnswer::Held, true) => {
                if book.set_locked(&key, true)
                    && let Some(note) = book.announcement(&key)
                {
                    edge.post(&note);
                    self.journal_hold(format!("hold: {key} lock is this tab's now"));
                }
                return;
            }
            (ClaimAnswer::Taken, true) => return,
            (ClaimAnswer::Held, false) => {
                book.hold(key, level.clone());
            }
            // The link is open here: over USB the OS already says this tab
            // has the board, and the network slot went to this tab, the
            // newest client. A lock someone else holds is one being let go
            // (the tab this one took the slot from yields) or a stale one:
            // announce the hold all the same, unlocked, and claim again
            // until it is ours.
            (ClaimAnswer::Taken, false) => {
                book.hold(key, level.clone());
                book.set_locked(&key, false);
                self.journal_hold(format!(
                    "hold: {key} lock busy; holding it unlocked and asking again"
                ));
            }
            (ClaimAnswer::Unavailable, _) => {
                self.board_hold_flow.unguarded.insert(key);
                return;
            }
        }
        if let Some(note) = self
            .board_hold_book
            .as_ref()
            .and_then(|book| book.announcement(&key))
        {
            edge.post(&note);
        }
        self.journal_hold(format!("hold: holding {key} ({level:?})"));
    }

    /// A board this tab held (or was claiming) is no longer open here: the
    /// port is closed already, so the lock goes, then the announcement.
    fn let_go_of_hold(&mut self, key: HoldKey) {
        let Some(edge) = self.board_hold_edge.clone() else {
            return;
        };
        self.board_hold_flow.unguarded.remove(&key);
        if self.board_hold_flow.claim_wanted(&key) {
            // In flight: the edge lets it go when it lands.
            self.board_hold_flow.claiming.insert(key, false);
            edge.release(&key);
            return;
        }
        if self.board_hold_flow.claiming.contains_key(&key) {
            return;
        }
        let held = self
            .board_hold_book
            .as_mut()
            .is_some_and(|book| book.let_go(&key));
        if held {
            edge.release(&key);
            edge.post(&HoldNote::Gone { key });
            self.journal_hold(format!("hold: let go of {key} (its port closed)"));
        }
    }

    /// Answer another tab's ask: refuse (busy, or not held), or start
    /// letting the board go.
    fn answer_hold_ask(
        &mut self,
        asker: TabId,
        request: u64,
        key: HoldKey,
        level: Option<HoldLevel>,
    ) {
        let Some(edge) = self.board_hold_edge.clone() else {
            return;
        };
        // The book's level is the last one announced; the board's own state
        // now is the fresher word (a push may have started this batch).
        let level = level.map(|said| self.desired_board_holds().remove(&key).unwrap_or(said));
        let releasing = self.board_hold_flow.releases.contains_key(&key);
        match answer_plan(level.as_ref(), releasing) {
            AnswerPlan::Refuse(refusal) => {
                self.journal_hold(format!("hold: asked for {key}; refused ({refusal:?})"));
                edge.post(&HoldNote::Answer {
                    request,
                    asker,
                    outcome: AskOutcome::Refused(refusal),
                });
            }
            AnswerPlan::Release => {
                let device = self.device_holding(&key);
                if device.is_some()
                    && self
                        .pool
                        .attached_session()
                        .map(|session| session.attachment().device)
                        == device
                {
                    self.close_device_lens();
                }
                self.journal_hold(format!("hold: asked for {key}; letting it go"));
                self.board_hold_flow.releases.insert(
                    key,
                    PendingRelease {
                        request,
                        asker,
                        key,
                        device,
                        stage: ReleaseStage::WriteFrame,
                    },
                );
            }
        }
    }

    /// The releases whose port has closed (or was waited on long enough):
    /// the lock goes, then `Released` to the asker and `Gone` to everyone,
    /// and this tab's board wears "taken by another tab".
    fn finish_due_hold_releases(&mut self, now: f64) {
        let Some(edge) = self.board_hold_edge.clone() else {
            return;
        };
        let ready: Vec<HoldKey> = self
            .board_hold_flow
            .releases
            .values()
            .filter(|release| {
                let closed = release
                    .device
                    .and_then(|device| self.devices.roster().device(device))
                    .is_none_or(|device| !device.evidence.presence.is_open());
                release.ready_to_release(closed, now)
            })
            .map(|release| release.key)
            .collect();
        for key in ready {
            let Some(release) = self.board_hold_flow.releases.remove(&key) else {
                continue;
            };
            if let Some(book) = self.board_hold_book.as_mut() {
                book.let_go(&key);
            }
            edge.release(&key);
            edge.post(&HoldNote::Answer {
                request: release.request,
                asker: release.asker,
                outcome: AskOutcome::Released,
            });
            edge.post(&HoldNote::Gone { key });
            self.board_hold_flow
                .taken_from_here
                .insert(key.mac(), key.via());
            self.journal_hold(format!("hold: released {key} to another tab"));
        }
    }

    /// The roster device whose link `key` names: the board's USB port, or
    /// its network session.
    fn device_holding(&self, key: &HoldKey) -> Option<crate::DeviceId> {
        let roster = self.devices.roster();
        roster
            .devices()
            .iter()
            .find(|device| {
                device
                    .link()
                    .and_then(|link| roster.link_info(link))
                    .is_some_and(|info| {
                        HoldKey::of_link(key.mac(), info) == Some(*key)
                            && device.identity.mac.as_ref().and_then(BoardKey::from_mac)
                                == Some(key.mac())
                    })
            })
            .map(|device| device.id)
    }

    /// The link this tab reaches the board with `mac` by over its network
    /// slot (the LAN or the relay), when it has one in the roster.
    fn network_link_of(&self, mac: BoardKey) -> Option<LinkInfo> {
        let roster = self.devices.roster();
        roster.devices().iter().find_map(|device| {
            if device.identity.mac.as_ref().and_then(BoardKey::from_mac) != Some(mac) {
                return None;
            }
            let info = roster.link_info(device.link()?)?;
            is_network_road(info).then(|| info.clone())
        })
    }

    /// Another tab says it holds the board `key` names by its network slot,
    /// and it is new to say so: the board gave the slot to that tab (the
    /// newest client that proves the holder's key) and closed this tab's
    /// session, or is about to. Yield it: close this tab's session BY
    /// REQUEST — so `browser_websocket.js` does not redial and take the
    /// slot back — and the board wears "taken by another tab". This tab
    /// holds nothing and reopens nothing; Connect is the person's.
    ///
    /// The session is closed through its link when the roster still has one
    /// that is open, or opening (`Action::Disconnect`, intent Disconnected).
    /// A session the board already dropped has no link here, and redials on
    /// its own: the transport is told to stop reaching the board there (the
    /// road it came by, [`BoardHoldFlow::network_roads`]). A link closed by
    /// request already needs nothing.
    ///
    /// [`BoardHoldFlow::network_roads`]: crate::app::devices::board_hold::BoardHoldFlow::network_roads
    fn yield_network_board(&mut self, key: HoldKey) {
        if self.board_hold_flow.releases.contains_key(&key) {
            // Already letting it go to an ask; that flow says the rest.
            return;
        }
        let mac = key.mac();
        let roster = self.devices.roster();
        let on_network: Vec<(crate::DeviceId, bool)> = roster
            .devices()
            .iter()
            .filter(|device| device.identity.mac.as_ref().and_then(BoardKey::from_mac) == Some(mac))
            .filter_map(|device| {
                let info = roster.link_info(device.link()?)?;
                let live = device.evidence.presence.is_open()
                    || device.intent.connection != ConnectionIntent::Disconnected;
                is_network_road(info).then_some((device.id, live))
            })
            .collect();
        let road = self.board_hold_flow.network_roads.remove(&mac);
        let mut closed = false;
        for (device, live) in &on_network {
            if *live {
                self.fold_device_input(DeviceInput::Action(DeviceAction::Disconnect {
                    device: *device,
                }));
                closed = true;
            }
        }
        if on_network.is_empty()
            && let Some(info) = road
        {
            self.stop_network_session(info);
            closed = true;
        }
        // The hold is over now: the lock goes and `Gone` is said (the tab
        // that took the slot claims the lock as it frees), and nothing
        // claims it again while the link closes.
        let held = self
            .board_hold_book
            .as_ref()
            .is_some_and(|book| book.holds(&key).is_some())
            || self.board_hold_flow.claiming.contains_key(&key);
        if held {
            self.let_go_of_hold(key);
            self.board_hold_flow.yielded.insert(key);
        }
        if closed || held {
            self.board_hold_flow
                .taken_from_here
                .insert(mac, HoldVia::Network);
            self.journal_hold(format!(
                "hold: another tab took {key}; this tab's session closes by request (no redial)"
            ));
        }
    }

    /// Stop reaching a board on the network road `info` names: its session
    /// closes by request and is not redialled (the transport's forget, the
    /// same one the card's Forget uses for the session).
    fn stop_network_session(&self, info: LinkInfo) {
        let stop = match info.endpoint.is_relay() {
            true => self
                .relay_transport
                .as_ref()
                .map(|transport| transport.revoke_grant(info)),
            false => self
                .lan_transport
                .as_ref()
                .map(|transport| transport.revoke_grant(info)),
        };
        if let (Some(stop), Some(spawner)) = (stop, self.wifi_spawner.clone()) {
            spawner(Box::pin(async move {
                if let Err(error) = stop.await {
                    log::warn!("hold: a network session did not stop: {error}");
                }
            }));
        }
    }

    /// Another tab's hold on `key` ended: the ports of its kind leave the
    /// gate (nothing opens them), the fact clears, and an ask waiting on
    /// it may open the board.
    ///
    /// Once no claim of that kind stands, the ports the claims kept shut
    /// lose their "held by another tab" mark (`Event::LinkFreed`), so a
    /// pending card stops saying "open in another Studio tab". With another
    /// board of the kind still held, the marks stay: which port was the
    /// freed board's is not known (two of a kind, a known limit).
    ///
    /// `stale`: the hold was read off the lock manager at load and its
    /// holder never spoke — the old page of a reload. The ports it kept
    /// shut open now, as a fresh load opens every granted port. Any other
    /// hold that ends opens nothing (R3): Connect is the person's.
    fn hold_freed(&mut self, key: HoldKey, stale: bool) {
        let mut released = Vec::new();
        if let Some(pair) = key.usb_pair() {
            released = self
                .devices
                .effects()
                .hold_gate()
                .borrow_mut()
                .release_pair(pair);
            let kind_still_held = self
                .board_hold_book
                .as_ref()
                .is_some_and(|book| book.claims_for_usb(pair.vendor, pair.product) > 0);
            if !kind_still_held {
                for link in self.links_marked_held(pair) {
                    self.fold_device_input(DeviceInput::Event(DeviceEvent::LinkFreed { link }));
                }
            }
        }
        let still_held = self
            .board_hold_book
            .as_ref()
            .is_some_and(|book| book.others_for_mac(key.mac()).next().is_some());
        if !still_held {
            self.board_hold_flow.taken_from_here.remove(&key.mac());
        }
        self.journal_hold(format!("hold: {key} is free"));
        let asked = !self.take_overs.asking_for(&key).is_empty();
        self.take_over_freed(key);
        if stale && !asked && !released.is_empty() {
            self.journal_hold(format!(
                "hold: {key} was a page that is gone (it never spoke); opening its ports as a \
                 fresh load does"
            ));
            self.open_released_ports(&released);
        }
    }

    /// Open `ports`, which the hold gate kept shut, the way a fresh load
    /// opens a granted port: a board's own link connects, and a pending
    /// port identifies. The OS lets only a free one open.
    fn open_released_ports(&mut self, ports: &[LinkId]) {
        let roster = self.devices.roster();
        let boards: Vec<crate::DeviceId> = roster
            .devices()
            .iter()
            .filter(|device| {
                device.link().is_some_and(|link| ports.contains(&link))
                    && !device.evidence.presence.is_open()
            })
            .map(|device| device.id)
            .collect();
        let pending: Vec<crate::DeviceId> = roster
            .pending()
            .iter()
            .filter(|pending| ports.contains(&pending.link))
            .map(|pending| pending.device_id())
            .collect();
        for device in boards {
            self.fold_device_input(DeviceInput::Action(DeviceAction::Connect { device }));
        }
        for device in pending {
            self.fold_device_input(DeviceInput::Action(DeviceAction::Identify { device }));
        }
    }

    /// This tab's links of `pair` that carry the "held by another tab"
    /// mark: pending ports and boards' own links alike.
    fn links_marked_held(&self, pair: UsbPair) -> Vec<LinkId> {
        let roster = self.devices.roster();
        let pending = roster
            .pending()
            .iter()
            .filter(|pending| {
                pending.evidence().link_held_by_tab() && usb_pair_of(&pending.info) == Some(pair)
            })
            .map(|pending| pending.link);
        let devices = roster.devices().iter().filter_map(|device| {
            let link = device.link()?;
            (device.evidence.link_held_by_tab()
                && roster.link_info(link).and_then(usb_pair_of) == Some(pair))
            .then_some(link)
        });
        pending.chain(devices).collect()
    }

    /// One sentinel per hold elsewhere; none for a hold that went.
    fn sync_hold_sentinels(&mut self) {
        let (Some(edge), Some(spawner), Some(tx)) = (
            self.board_hold_edge.clone(),
            self.wifi_spawner.clone(),
            self.wifi_tx.clone(),
        ) else {
            return;
        };
        // Only a locked hold: an unlocked one's lock coming free says nothing
        // about its holder (it is the lock of the tab it took the board
        // from). And not a hold this tab has itself: its own lock is no other
        // tab's to let go (a board's network slot changes hands before the
        // tab it left has said `Gone`).
        let others: BTreeSet<HoldKey> = self
            .board_hold_book
            .as_ref()
            .map(|book| {
                book.others()
                    .filter(|(key, hold)| hold.locked && book.holds(key).is_none())
                    .map(|(key, _)| *key)
                    .collect()
            })
            .unwrap_or_default();
        let stale: Vec<HoldKey> = self
            .board_hold_flow
            .watching
            .keys()
            .filter(|key| !others.contains(key))
            .copied()
            .collect();
        for key in stale {
            self.board_hold_flow.watching.remove(&key);
            edge.unwatch(&key);
        }
        for key in others {
            if self.board_hold_flow.watching.contains_key(&key) {
                continue;
            }
            let watch = self.board_hold_flow.mint_watch();
            self.board_hold_flow.watching.insert(key, watch);
            let sentinel = edge.watch(&key);
            let tx = tx.clone();
            spawner(Box::pin(async move {
                let freed = sentinel.await;
                tx.send(StudioCommand::HoldEdge(HoldEdgeEvent::WatchEnded {
                    key,
                    watch,
                    freed,
                }));
            }));
        }
    }

    /// Put the other tabs' USB claims on the effects layer's gate.
    fn sync_hold_gate_claims(&mut self) {
        let mut claims: BTreeMap<UsbPair, Vec<lpa_devices::MacAddress>> = BTreeMap::new();
        if let Some(book) = self.board_hold_book.as_ref() {
            for (key, _) in book.others() {
                if let Some(pair) = key.usb_pair() {
                    claims.entry(pair).or_default().push(key.mac_address());
                }
            }
        }
        self.devices
            .effects()
            .hold_gate()
            .borrow_mut()
            .set_claims(claims);
    }

    /// Read every refused open against the claims: a pending USB link whose
    /// identify settled Failed with its port never open, where this tab's
    /// held ports of its kind number no more than the claims, is another
    /// tab's — `Event::LinkHeld`, naming the board when exactly one claim
    /// and one port of the kind are in play. Otherwise the model's own
    /// words stand ("in use by another app or another Studio tab").
    fn read_refused_ports(&mut self) {
        let gate = Rc::clone(self.devices.effects().hold_gate());
        let mut refused: BTreeMap<UsbPair, Vec<LinkId>> = BTreeMap::new();
        for pending in self.devices.roster().pending() {
            let Some(pair) = usb_pair_of(&pending.info) else {
                continue;
            };
            let evidence = pending.evidence();
            let settled_failed =
                matches!(evidence.last_outcome, Some(ActivityOutcome::Failed { .. }));
            if pending.is_identifying()
                || !settled_failed
                || evidence.presence.is_open()
                || evidence.link_held_by_tab()
                || gate.borrow().holds(pending.link)
            {
                continue;
            }
            refused.entry(pair).or_default().push(pending.link);
        }
        for (pair, links) in refused {
            let (claims, held) = {
                let gate = gate.borrow();
                (gate.claims_for(pair), gate.held_count(pair))
            };
            if !reads_as_held(held + links.len(), claims) {
                continue;
            }
            for link in &links {
                gate.borrow_mut().mark_read_held(*link, pair);
            }
            for link in links {
                let mac = gate.borrow().association(link);
                self.journal_hold(format!(
                    "hold: port {} was refused: another tab holds it",
                    link.0
                ));
                self.fold_device_input(DeviceInput::Event(DeviceEvent::LinkHeld { link, mac }));
            }
        }
    }

    /// A held port that named no board when it was held may be named now
    /// (the claims changed): say which board it is, so it merges onto that
    /// board's card.
    fn name_held_ports(&mut self) {
        let gate = Rc::clone(self.devices.effects().hold_gate());
        let named: Vec<(LinkId, lpa_devices::MacAddress)> = self
            .devices
            .roster()
            .pending()
            .iter()
            .filter(|pending| {
                pending.evidence().link_held_by_tab() && pending.identity().mac.is_none()
            })
            .filter_map(|pending| Some((pending.link, gate.borrow().association(pending.link)?)))
            .collect();
        for (link, mac) in named {
            self.fold_device_input(DeviceInput::Event(DeviceEvent::LinkHeld {
                link,
                mac: Some(mac),
            }));
        }
    }

    /// Put the fact on every board another tab holds, and take it off the
    /// ones it no longer holds: only what changed is folded.
    fn put_hold_facts_on_boards(&mut self) {
        let Some(book) = self.board_hold_book.as_ref() else {
            return;
        };
        // A hold this tab has itself is not another tab's, whatever the book
        // still lists for it (the tab it took the network slot from has not
        // said `Gone` yet).
        let others = book.others().filter(|(key, _)| book.holds(key).is_none());
        let desired = desired_facts(others, &self.board_hold_flow.taken_from_here);
        let changes = fact_changes(&desired, &self.board_hold_flow.facts_sent);
        for (mac, held) in changes {
            match &held {
                Some(fact) => {
                    self.board_hold_flow.facts_sent.insert(mac, fact.clone());
                }
                None => {
                    self.board_hold_flow.facts_sent.remove(&mac);
                }
            }
            self.fold_device_input(DeviceInput::Event(DeviceEvent::BoardHeld {
                mac: mac.to_mac_address(),
                held,
            }));
        }
    }

    /// Wake the hold flows after `secs` (a deadline they wait on), on the
    /// device layer's timer.
    pub(super) fn wake_board_holds_after(&self, secs: f64) {
        let (Some(spawner), Some(tx), Some(timer)) = (
            self.wifi_spawner.clone(),
            self.wifi_tx.clone(),
            self.devices.effects().timer_factory(),
        ) else {
            return;
        };
        let sleep = (timer.borrow_mut())(core::time::Duration::from_secs_f64(secs.max(0.0)));
        spawner(Box::pin(async move {
            sleep.await;
            tx.send(StudioCommand::HoldEdge(HoldEdgeEvent::Due));
        }));
    }

    /// One plain-words line in the device journal, like the `wire:` notes.
    pub(super) fn journal_hold(&self, entry: String) {
        log::debug!("{entry}");
        self.record_device_event(
            None,
            None,
            DeviceEventKind::Journal {
                scope: HOLD_JOURNAL_SCOPE.to_string(),
                entry,
            },
        );
    }
}
