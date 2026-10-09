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
//! - **Claims.** A USB board whose port is open here and whose hello named
//!   its MAC is claimed (open, hello, then lock, then announce) and
//!   announced at its level; a board whose port closed is let go (the port
//!   is closed already, then the lock, then the announcement).
//! - **The gate.** The claims of other tabs keep the effects layer's gate
//!   current, and a port whose open the OS refused is read against them.
//! - **The answer.** An ask is refused while busy; otherwise the lens
//!   closes, the last picture is written, the link disconnects, its close
//!   is awaited, the lock goes, and `Released` and `Gone` are said.
//! - **The sentinel.** One watch per hold elsewhere: when it fires the
//!   holder let go or died, the fact clears — and nothing opens the port.
//! - **The facts.** Every board another tab holds wears the fact
//!   (`Event::BoardHeld`), folded only when it changed.
//!
//! Async answers come back as `StudioCommand::HoldEdge` on the actor's
//! queue ([`StudioController::on_hold_edge_event`]); time is the injected
//! clock, and the deadlines are woken by the device layer's timer.

use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use lpa_devices::activity::{ActivityKind, ActivityOutcome};
use lpa_devices::link::LinkId;
use lpa_devices::{BoardKey, HoldLevel};

use super::StudioController;
use crate::app::devices::board_hold::{
    AnswerPlan, AskOutcome, BookChange, ClaimAnswer, HoldCandidate, HoldEdgeEvent, HoldKey,
    HoldNote, HoldPriming, PRIMING_PATIENCE_SECS, PendingRelease, RELEASE_CLOSE_PATIENCE_SECS,
    ReleaseStage, TabId, UsbPair, answer_plan, desired_facts, desired_holds, fact_changes,
    plan_holds, reads_as_held, usb_pair_of,
};
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
                    let change = self
                        .board_hold_book
                        .as_mut()
                        .and_then(|book| book.freed_by_watch(&key));
                    if let Some(change) = change {
                        self.journal_hold(format!(
                            "hold: {key} came free (its tab let go or closed)"
                        ));
                        self.react_to_hold_changes(&[change]);
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
                BookChange::HeldElsewhere { key, level, .. } => {
                    self.journal_hold(format!(
                        "hold: {key} is held by another tab ({})",
                        level
                            .as_ref()
                            .map_or("level not said yet".to_string(), |level| format!(
                                "{level:?}"
                            ))
                    ));
                }
                BookChange::Freed { key } => self.hold_freed(*key),
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

        // This tab's own holds.
        let desired = self.desired_board_holds();
        let answering: BTreeSet<HoldKey> =
            self.board_hold_flow.releases.keys().copied().collect();
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
            {
                edge.post(&HoldNote::Holds {
                    key,
                    level: level.clone(),
                });
                self.journal_hold(format!("hold: {key} now {level:?}"));
            }
        }
        for key in plan.claim {
            self.claim_board_hold(key);
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

    /// The holds this tab should have now: every roster board whose USB
    /// port is open here and whose MAC is known, at its level.
    fn desired_board_holds(&self) -> BTreeMap<HoldKey, HoldLevel> {
        let now = self.device_now();
        let lens = self
            .pool
            .attached_session()
            .map(|session| session.attachment().device);
        let roster = self.devices.roster();
        desired_holds(roster.devices().iter().map(|device| {
            let view = lpa_devices::view::device_view(device, now);
            HoldCandidate {
                mac: device.identity.mac.as_ref().and_then(BoardKey::from_mac),
                usb: device
                    .link()
                    .and_then(|link| roster.link_info(link))
                    .and_then(usb_pair_of),
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
            tx.send(StudioCommand::HoldEdge(HoldEdgeEvent::Claimed { key, answer }));
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
        match answer {
            ClaimAnswer::Held => {}
            // The port is open here, so the OS already says this tab has
            // the board; a lock someone else holds is a stale one.
            ClaimAnswer::Taken => {
                self.journal_hold(format!("hold: {key} lock busy; continuing unguarded"))
            }
            ClaimAnswer::Unavailable => {
                self.board_hold_flow.unguarded.insert(key);
                return;
            }
        }
        if let Some(book) = self.board_hold_book.as_mut() {
            book.hold(key, level.clone());
        }
        edge.post(&HoldNote::Holds {
            key,
            level: level.clone(),
        });
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

    /// The roster device whose open port `key` names.
    fn device_holding(&self, key: &HoldKey) -> Option<crate::DeviceId> {
        let roster = self.devices.roster();
        roster
            .devices()
            .iter()
            .find(|device| {
                device.identity.mac.as_ref().and_then(BoardKey::from_mac) == Some(key.mac())
                    && device
                        .link()
                        .and_then(|link| roster.link_info(link))
                        .and_then(usb_pair_of)
                        == key.usb_pair()
            })
            .map(|device| device.id)
    }

    /// Another tab's hold on `key` ended: the ports of its kind leave the
    /// gate (nothing opens them), the fact clears, and an ask waiting on
    /// it may open the board.
    fn hold_freed(&mut self, key: HoldKey) {
        if let Some(pair) = key.usb_pair() {
            self.devices
                .effects()
                .hold_gate()
                .borrow_mut()
                .release_pair(pair);
        }
        let still_held = self
            .board_hold_book
            .as_ref()
            .is_some_and(|book| book.others_for_mac(key.mac()).next().is_some());
        if !still_held {
            self.board_hold_flow.taken_from_here.remove(&key.mac());
        }
        self.journal_hold(format!("hold: {key} is free"));
        self.take_over_freed(key);
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
        let others: BTreeSet<HoldKey> = self
            .board_hold_book
            .as_ref()
            .map(|book| book.others().map(|(key, _)| *key).collect())
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
            let settled_failed = matches!(
                evidence.last_outcome,
                Some(ActivityOutcome::Failed { .. })
            );
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
        let desired = desired_facts(book.others(), &self.board_hold_flow.taken_from_here);
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
