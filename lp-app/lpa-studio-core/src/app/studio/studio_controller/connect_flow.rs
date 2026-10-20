//! Connected: the home page holds the open session.
//!
//! Connect (`devices/<board>/connect`), Edit (`devices/<board>/edit`) and
//! Done (`devices/<board>/done`) on the studio controller, and the one rule
//! for which surface an open session shows on
//! ([`crate::ConnectedBoard::shows_editor`]). The session itself is the
//! editor's lens, opened the way an address opens it
//! ([`StudioController::open_device_lens_for`]): this file records who holds
//! it, and nothing about the runtime pool changes.
//!
//! Connect reaches a board Studio is not talking to yet first — it opens a
//! closed port, or takes an offline board's road back (`ConnectReach`) —
//! and holds the intent ([`crate::PendingLens::connect`]) until the board is
//! ready, for [`crate::CONNECT_INTENT_GRACE`] at most
//! ([`StudioController::try_connect_hold`], from the refresh tick).

use lpa_devices::DeviceId;

use crate::{
    ConnectFailure, ConnectPhase, ConnectedBoard, RuntimeId, StudioController, UiError, UiNotices,
    UiOffer, UiResult, UxUpdateSink,
};

/// What a connect asks of the open: the home page holds the session, and
/// an Edit may be waiting for the editor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct ConnectIntent {
    /// Edit was pressed: the editor shows until the user reaches the
    /// session's page.
    pub(super) editor_waiting: bool,
}

impl ConnectIntent {
    /// The record of this connect's session `id` on `device` (`uid`), as
    /// it opens.
    pub(super) fn opening(self, id: RuntimeId, device: DeviceId, uid: &str) -> ConnectedBoard {
        ConnectedBoard {
            session: id,
            device,
            uid: uid.to_string(),
            editor_waiting: self.editor_waiting,
            phase: ConnectPhase::Opening,
        }
    }
}

impl StudioController {
    /// The connected session: the home page holds it. `None` when nothing
    /// is connected, and when the recorded session has left the pool by
    /// any road ([`ConnectedBoard::rides`]), so no caller reads a stale
    /// record.
    pub fn connected(&self) -> Option<&ConnectedBoard> {
        let attached = self.pool.attached_session().map(crate::RuntimeSession::id);
        self.connected
            .as_ref()
            .filter(|connected| connected.rides(attached))
    }

    fn connected_mut(&mut self) -> Option<&mut ConnectedBoard> {
        let attached = self.pool.attached_session().map(crate::RuntimeSession::id);
        self.connected
            .as_mut()
            .filter(|connected| connected.rides(attached))
    }

    /// Why the last Connect did not open, until the next Connect, Edit or
    /// Done.
    pub fn connect_failure(&self) -> Option<&ConnectFailure> {
        self.connect_failure.as_ref()
    }

    /// Whether a connected session shows on its board's card (so the home
    /// page stays up with the session's project open): no Edit is waiting
    /// and the user is not on the session's own page.
    pub(super) fn connected_shows_on_its_card(&self) -> bool {
        let lens_project = self.project.active_library_uid();
        self.connected().is_some_and(|connected| {
            !connected.shows_editor(self.place.as_ref(), lens_project.as_deref())
        })
    }

    /// The web reported `place`, the user having been at the place this
    /// controller last heard: a waiting Edit ends at the first report that
    /// moves the user to another page ([`ConnectedBoard::note_page_moved`]).
    /// Read-only, like every use of the place: nothing opens or closes.
    pub(super) fn note_place_for_connected(&mut self, place: &crate::UiPlace) {
        let before = self.place.as_ref().map(|place| place.page.clone());
        if let Some(connected) = self.connected_mut() {
            connected.note_page_moved(before.as_ref(), &place.page);
        }
    }

    /// Where this tab's session stands on each board it is about, for the
    /// cards (`DeviceRosterView.connections`): the last Connect's failure,
    /// a Connect held for its board, and the connected session —
    /// Connecting while it opens, Reconnecting while its dropped link is
    /// held, else Connected. Every other board is watched (absent).
    pub(super) fn board_connections(
        &self,
    ) -> std::collections::BTreeMap<DeviceId, crate::BoardConnection> {
        let mut connections = std::collections::BTreeMap::new();
        if let Some(failure) = &self.connect_failure {
            connections.insert(
                failure.device,
                crate::BoardConnection::Failed {
                    reason: failure.reason.clone(),
                },
            );
        }
        if let Some(board) = self
            .pending_device_lens
            .as_ref()
            .filter(|pending| pending.connect.is_some())
            .and_then(|pending| self.device_at_address(&pending.uid))
        {
            connections.insert(board.id, crate::BoardConnection::Connecting);
        }
        if let Some(connected) = self.connected() {
            let held = self
                .lens_hold
                .as_ref()
                .is_some_and(|hold| hold.uid == connected.uid);
            let state = match (held, connected.phase) {
                (true, _) => crate::BoardConnection::Reconnecting,
                (false, ConnectPhase::Opening) => crate::BoardConnection::Connecting,
                (false, ConnectPhase::Open) => crate::BoardConnection::Connected,
            };
            connections.insert(connected.device, state);
        }
        connections
    }

    /// The connected board's panel at card size, while its session shows
    /// on its card and its project is ready: the project's root panel
    /// ([`crate::ProjectEditorView::root_module_face`]) picked by
    /// [`crate::board_panel_picks`]. The editor's view is built into a
    /// scratch offer tree, so none of the editor's node verbs are published
    /// on the home page; the editor's own view is untouched.
    pub(super) fn connected_board_panel(&self) -> Option<(DeviceId, crate::UiBoardPanel)> {
        let connected = self.connected()?;
        if connected.phase != ConnectPhase::Open || !self.connected_shows_on_its_card() {
            return None;
        }
        let mut scratch = crate::UiOfferTree::new();
        let pane = self
            .project
            .view(self.has_lightplayer_state(), &mut scratch);
        let crate::UiViewContent::ProjectEditor(editor) = &pane.body else {
            return None;
        };
        let face = editor.root_module_face()?;
        Some((
            connected.device,
            crate::board_panel_picks(&face.panel, face.auto_save),
        ))
    }

    /// The lens on session `id` attached: a connect's open is over.
    pub(super) fn note_connected_open(&mut self, id: RuntimeId) {
        if let Some(connected) = self.connected_mut()
            && connected.session == id
        {
            connected.phase = ConnectPhase::Open;
        }
    }

    /// `RuntimeOp::OpenDeviceLens`, an address's open (`/device/<uid>`):
    /// on the board a connected session already holds it does nothing — no
    /// reattach, no fresh mirror — because the user being on the session's
    /// own address is what shows it in the editor. Anywhere else it is
    /// today's open.
    pub(super) async fn open_address_lens(&mut self, uid: &str, updates: UxUpdateSink) -> UiResult {
        if self
            .connected()
            .is_some_and(|connected| connected.uid == uid)
        {
            return Ok(UiNotices::new());
        }
        self.open_device_lens(uid, updates).await
    }

    /// `RuntimeOp::ConnectDevice` (and Edit's connect-first, with
    /// `editor_waiting`): open `device`'s session held by the home page,
    /// reaching the board first when it must.
    ///
    /// - Already connected here: a Connect has nothing to do; an Edit shows
    ///   the editor on the session open now, reattaching and reading
    ///   nothing.
    /// - A session an address opened on this board: the home page takes it
    ///   over as it is, with nothing reopened.
    /// - The port is there but closed: the port opens (today's action,
    ///   face-aware) and the Connect holds until the board is ready —
    ///   except while an editor open is held for the board (the opening
    ///   frame's exit), when it opens the port only and the held open goes
    ///   on to the editor.
    /// - Offline, with `reach`: the board is reached the way that road's own
    ///   offer reaches it, and the Connect holds.
    /// - Otherwise the lens opens the way an address opens it (holding, if
    ///   the board is not ready yet).
    ///
    /// Every road but the held editor open's hands over first: a session on
    /// another board closes and its card shows its facts at once (one board
    /// connected at a time), and a held open elsewhere is let go — a Connect
    /// never sends a project anywhere. A connect that cannot open leaves its
    /// reason for the board's card.
    pub(super) async fn connect_device(
        &mut self,
        device: DeviceId,
        reach: Option<crate::ConnectReach>,
        editor_waiting: bool,
        updates: UxUpdateSink,
    ) -> UiResult {
        self.connect_failure = None;
        if let Some(connected) = self.connected_mut()
            && connected.device == device
        {
            connected.editor_waiting |= editor_waiting;
            self.drop_connect_hold();
            self.mark_dirty();
            return Ok(UiNotices::new());
        }
        if let Some((id, uid)) = self
            .pool
            .attached_session()
            .filter(|session| session.attachment().device == device)
            .map(|session| (session.id(), session.attachment().uid.clone()))
        {
            self.drop_connect_hold();
            self.connected = Some(ConnectedBoard {
                session: id,
                device,
                uid,
                editor_waiting,
                phase: ConnectPhase::Open,
            });
            self.mark_dirty();
            return Ok(UiNotices::new());
        }
        let uid = self.devices.key_for(device).map(str::to_string);
        let status = self
            .devices
            .roster()
            .device(device)
            .map(|board| lpa_devices::view::device_view(board, self.device_now()).status);
        let intent = ConnectIntent { editor_waiting };
        if status == Some(lpa_devices::device::DeviceStatus::Attached) {
            let held_open = uid.as_deref().is_some_and(|uid| {
                self.pending_device_lens
                    .as_ref()
                    .is_some_and(|pending| pending.is_address_for(uid))
            });
            if held_open {
                return self.open_port(device, uid.as_deref()).await;
            }
            self.hand_over_from(device);
            self.open_port(device, uid.as_deref()).await?;
            if let Some(uid) = uid {
                self.hold_connect(&uid, intent, &"the port is opening");
            }
            return Ok(UiNotices::new());
        }
        let Some(uid) = uid else {
            return Err(self.connect_failed(
                device,
                UiError::UnsupportedAction("this board has not said who it is yet".to_string()),
            ));
        };
        if status == Some(lpa_devices::device::DeviceStatus::Offline)
            && let Some(reach) = reach
        {
            self.hand_over_from(device);
            let chooser_before = self.devices.effects().last_usb_grant_request();
            if let Err(error) = self.reach_board(device, reach).await {
                return Err(self.connect_failed(device, error));
            }
            self.hold_connect(&uid, intent, &"the board is being reached");
            // The cable's road opened the port chooser: the hold waits on
            // its answer too (`note_connect_chooser`).
            if let Some(link) = self
                .devices
                .effects()
                .last_usb_grant_request()
                .filter(|link| Some(*link) != chooser_before)
            {
                self.pending_device_lens = self
                    .pending_device_lens
                    .take()
                    .map(|pending| pending.with_chooser(link));
            }
            return Ok(UiNotices::new());
        }
        self.let_a_held_open_go();
        match self.open_device_lens_for(&uid, Some(intent), updates).await {
            Ok(notices) => Ok(notices),
            Err(error) => Err(self.connect_failed(device, error)),
        }
    }

    /// `RuntimeOp::EditDevice`: the editor on `device`'s session — the one
    /// open now when it is connected, else a connect first.
    pub(super) async fn edit_device(
        &mut self,
        device: DeviceId,
        updates: UxUpdateSink,
    ) -> UiResult {
        self.connect_device(device, None, true, updates).await
    }

    /// Look at a Connect's hold on `uid` (from the refresh tick): attach it
    /// on the card once the board is ready, the way a Connect on a ready
    /// board does; give up after the grace, saying so on the card; let go
    /// at once when the board turns out locked or in need of firmware, whose
    /// own primary (Unlock, Install) takes over.
    pub(super) async fn try_connect_hold(&mut self, uid: String, hold: crate::ConnectHold) {
        let now = (self.now_secs)();
        let device = self.device_at_address(&uid).map(|board| board.id);
        if hold.expired(now) {
            self.pending_device_lens = None;
            if let Some(device) = device {
                self.connect_failure =
                    Some(self.connect_failure_for(device, &crate::CONNECT_GAVE_UP));
            }
            self.push_log(crate::UiLogDraft::new(
                crate::UiLogLevel::Warn,
                crate::UiLogOrigin::Studio,
                format!(
                    "{uid} did not answer within {} s; the connect is given up",
                    crate::CONNECT_INTENT_GRACE.as_secs()
                ),
            ));
            self.mark_dirty();
            return;
        }
        if let Some(why) = self.connect_hold_yields(&uid) {
            self.pending_device_lens = None;
            self.push_log(crate::UiLogDraft::new(
                crate::UiLogLevel::Info,
                crate::UiLogOrigin::Studio,
                format!("{uid} {why}; the connect is let go"),
            ));
            self.mark_dirty();
            return;
        }
        // Not here, identifying, unlocking or busy: keep holding.
        if self.device_lens_attachment(&uid).is_err() {
            return;
        }
        let intent = ConnectIntent {
            editor_waiting: hold.editor_waiting,
        };
        if let Err(error) = self
            .open_device_lens_for(&uid, Some(intent), UxUpdateSink::noop())
            .await
        {
            let error = match device {
                Some(device) => self.connect_failed(device, error),
                None => error,
            };
            // A held connect lands from the tick, not from an action, so no
            // dispatch reports its error: a tier refusal raises the unlock
            // sheet here.
            self.note_action_error(&error);
        }
    }

    /// The chooser a Connect's hold opened answered for `link`: a board
    /// picked keeps the hold (it attaches on the tick, once ready); nothing
    /// picked lets it go at once, with no failure (the user said no, and
    /// the card is back at Connect); a chooser that failed lets it go and
    /// says why on the card. Any other chooser's answer is not the hold's.
    pub(super) fn note_connect_chooser(
        &mut self,
        link: lpa_devices::LinkId,
        answer: &lpa_devices::GrantAnswer,
    ) {
        let Some(pending) = self.pending_device_lens.as_ref().filter(|pending| {
            pending
                .connect
                .is_some_and(|hold| hold.chooser == Some(link))
        }) else {
            return;
        };
        let uid = pending.uid.clone();
        let device_at = self.device_at_address(&uid).map(|board| board.id);
        let why = match answer {
            lpa_devices::GrantAnswer::Picked => {
                if let Some(hold) = self
                    .pending_device_lens
                    .as_mut()
                    .and_then(|pending| pending.connect.as_mut())
                {
                    hold.chooser = None;
                }
                return;
            }
            lpa_devices::GrantAnswer::Dismissed => {
                "the chooser closed with nothing picked".to_string()
            }
            lpa_devices::GrantAnswer::Failed { error } => {
                let failure = device_at.map(|device| self.connect_failure_for(device, error));
                self.connect_failure = failure.or(self.connect_failure.take());
                format!("the chooser failed: {error}")
            }
        };
        self.pending_device_lens = None;
        self.push_log(crate::UiLogDraft::new(
            crate::UiLogLevel::Info,
            crate::UiLogOrigin::Studio,
            format!("{uid}: {why}; the connect is let go"),
        ));
        self.mark_dirty();
    }

    /// Connect a board's USB or Bluetooth square was pressed: the last
    /// failure's words go, and the chooser it opened (if it opened one; the
    /// latest request is newer than `chooser_before`) is the one whose
    /// answer the section waits on.
    pub(super) fn note_section_chooser(
        &mut self,
        action: &lpa_devices::Action,
        chooser_before: Option<lpa_devices::LinkId>,
    ) {
        let transport = match action {
            lpa_devices::Action::AddFromUsb => crate::ChooserTransport::Usb,
            lpa_devices::Action::AddFromBle => crate::ChooserTransport::Bluetooth,
            _ => return,
        };
        if self.chooser_failure.take().is_some() {
            self.mark_dirty();
        }
        self.section_chooser = self
            .devices
            .effects()
            .last_grant_request()
            .filter(|link| Some(*link) != chooser_before)
            .map(|link| (link, transport));
    }

    /// The chooser Connect a board opened answered for `link`. A board
    /// picked or nothing picked says nothing (the board's card, or the
    /// section as it was, says the rest); a connect that failed after the
    /// pick says so under the squares (defect 2026-10-17: it was a console
    /// warning). Any other chooser's answer is not the section's.
    pub(super) fn note_section_chooser_answer(
        &mut self,
        link: lpa_devices::LinkId,
        answer: &lpa_devices::GrantAnswer,
    ) {
        let Some((_, transport)) = self.section_chooser.filter(|(waits, _)| *waits == link) else {
            return;
        };
        self.section_chooser = None;
        if let lpa_devices::GrantAnswer::Failed { error } = answer {
            self.chooser_failure = Some(crate::UiChooserFailure::new(transport, error));
            self.mark_dirty();
        }
    }

    /// Why a Connect's hold on the board at `uid` lets go at once, while
    /// the board is here: it is locked (Unlock is its way in) or needs
    /// firmware (Install is). `None` while neither, or while it is not here
    /// to say.
    fn connect_hold_yields(&self, uid: &str) -> Option<&'static str> {
        let board = self.device_at_address(uid)?;
        if !board.evidence.presence.is_open() {
            return None;
        }
        if lpa_devices::view::device_view(board, self.device_now()).needs_firmware() {
            return Some("needs firmware");
        }
        let locked = self
            .access
            .device_view(board)
            .is_some_and(|access| access.unlock == Some(crate::UiUnlockOffer::Locked));
        locked.then_some("is locked")
    }

    /// Hold a Connect on `uid` until its board is ready (`why` is what it
    /// waits for, for the console). It replaces any other hold: one board
    /// is being connected at a time.
    pub(super) fn hold_connect(
        &mut self,
        uid: &str,
        intent: ConnectIntent,
        why: &dyn core::fmt::Display,
    ) {
        self.pending_device_lens = Some(crate::PendingLens::connect(
            uid,
            (self.now_secs)(),
            intent.editor_waiting,
        ));
        self.push_log(crate::UiLogDraft::new(
            crate::UiLogLevel::Info,
            crate::UiLogOrigin::Studio,
            format!("connecting {uid} once it is ready: {why}"),
        ));
        self.mark_dirty();
    }

    /// The lens open held for its board, if one is: an address's, or a
    /// Connect's ([`crate::PendingLens::connect`], the card's
    /// "Connecting…").
    pub fn pending_lens(&self) -> Option<&crate::PendingLens> {
        self.pending_device_lens.as_ref()
    }

    /// Let a Connect's hold go (an address's stays: it is its own).
    fn drop_connect_hold(&mut self) {
        if self
            .pending_device_lens
            .as_ref()
            .is_some_and(|pending| pending.connect.is_some())
        {
            self.pending_device_lens = None;
        }
    }

    /// Hand over to `device`: a session on another board closes now (its
    /// card shows its facts at once), and a held open elsewhere is let go.
    fn hand_over_from(&mut self, device: DeviceId) {
        if self
            .pool
            .attached_session()
            .is_some_and(|session| session.attachment().device != device)
        {
            self.close_device_lens();
        }
        self.let_a_held_open_go();
    }

    /// Open `device`'s closed port: today's action, in its face's words (a
    /// sim's is Power on).
    async fn open_port(&mut self, device: DeviceId, uid: Option<&str>) -> UiResult {
        let action = crate::DeviceAction::Connect { device };
        let op = match uid.is_some_and(|uid| self.is_runtime_device(uid)) {
            true => crate::DevicesOp::on_sim(action),
            false => crate::DevicesOp::new(action),
        };
        self.execute_devices_op(op).await
    }

    /// Reach an offline `device` by `reach`: the code its own offer runs
    /// (`connect-wifi`, `connect-relay`, `reconnect`). A Wi‑Fi try whose
    /// board lightplayer.app also reaches goes on there if it fails
    /// ([`Self::fall_through_to_relay`]).
    async fn reach_board(&mut self, device: DeviceId, reach: crate::ConnectReach) -> UiResult {
        match reach {
            crate::ConnectReach::Wifi => {
                let then_relay = self.board_key(device).is_some_and(|key| {
                    self.roads_to(key, false)
                        .contains(&crate::app::devices::best_link::Road::Relay)
                });
                self.start_wifi_connect(crate::WifiConnectOp::Board { device, then_relay })
            }
            crate::ConnectReach::Relay => {
                self.start_relay_connect(crate::RelayConnectOp { device })
            }
            crate::ConnectReach::Usb => {
                self.execute_devices_op(crate::DevicesOp::new(crate::DeviceAction::Reconnect {
                    device,
                }))
                .await
            }
        }
    }

    /// The roads to the board with MAC `key` from this page, best first
    /// ([`crate::app::devices::best_link::roads`]): Wi‑Fi while this page
    /// has a LAN transport, lightplayer.app while it has the relay's and
    /// someone is signed in, and the cable when `cable` says it can be
    /// asked for. Neither network road while another tab of this browser
    /// holds the board's network slot (its Connect is `take-over`).
    pub(super) fn roads_to(
        &self,
        key: lpa_devices::BoardKey,
        cable: bool,
    ) -> Vec<crate::app::devices::best_link::Road> {
        use crate::app::devices::best_link::{BestLinkFacts, Road, roads};
        let held_elsewhere = self.network_slot_held_elsewhere(key);
        let remembered = self
            .wifi_addresses
            .get(&key)
            .map(|address| address.ip.clone());
        let row = self.account_lan_hint(key);
        let facts = BestLinkFacts {
            linked_here: self.reaches_board(key),
            remembered: remembered.as_deref(),
            row: row.as_ref(),
            lan_reach: self.lan_reach,
            relay: self.relay_transport.is_some()
                && self.access.account_keys().is_some()
                && !held_elsewhere,
            cable,
        };
        let wifi = self.lan_transport.is_some() && !held_elsewhere;
        roads(&facts)
            .into_iter()
            .filter(|road| wifi || !matches!(road, Road::Wifi { .. }))
            .collect()
    }

    /// Where Studio dials the board with MAC `key` on Wi‑Fi when someone
    /// asks: the address lightplayer.app says it gave while it shares this
    /// network, else the one this browser remembers
    /// ([`crate::app::devices::best_link::wifi_address`]).
    pub(super) fn wifi_address_for(&self, key: lpa_devices::BoardKey) -> Option<String> {
        let remembered = self
            .wifi_addresses
            .get(&key)
            .map(|address| address.ip.clone());
        let row = self.account_lan_hint(key);
        crate::app::devices::best_link::wifi_address(&crate::BestLinkFacts {
            remembered: remembered.as_deref(),
            row: row.as_ref(),
            ..Default::default()
        })
    }

    /// The Wi‑Fi try of a Connect that goes on through lightplayer.app
    /// failed (`board` is in [`Self::fall_through`]): say so in one line and
    /// start the relay's, for the same board. The card's held Connect, if
    /// any, keeps holding; it ends on the relay's answer. `false` when the
    /// board was not falling through, or the relay could not start (the
    /// Wi‑Fi failure then stands on the card as usual).
    pub(super) fn fall_through_to_relay(&mut self, board: lpa_devices::BoardKey) -> bool {
        if !self.fall_through.contains(&board) {
            return false;
        }
        let Some(device) = self.device_for_board(board) else {
            self.fall_through.remove(&board);
            return false;
        };
        if self
            .start_relay_connect(crate::RelayConnectOp { device })
            .is_err()
        {
            self.fall_through.remove(&board);
            return false;
        }
        let name = self.board_title(board);
        self.push_log(crate::UiLogDraft::new(
            crate::UiLogLevel::Info,
            crate::UiLogOrigin::Studio,
            format!("Couldn't reach {name} over Wi\u{2011}Fi; trying lightplayer.app."),
        ));
        true
    }

    /// The relay's try of a Connect that fell through ended: the press is
    /// over. When it failed too, the card's held Connect lets go at once, so
    /// the card shows the relay's words rather than "Connecting…".
    pub(super) fn fall_through_ended(&mut self, board: lpa_devices::BoardKey, failed: bool) {
        if !self.fall_through.remove(&board) || !failed {
            return;
        }
        let held_here = self.pending_device_lens.as_ref().is_some_and(|pending| {
            pending.connect.is_some()
                && self
                    .device_at_address(&pending.uid)
                    .is_some_and(|device| self.board_key(device.id) == Some(board))
        });
        if held_here {
            self.drop_connect_hold();
            self.mark_dirty();
        }
    }

    /// The roster device (or the account's entry) the board with MAC `key`
    /// is, if Studio knows it.
    pub(super) fn device_for_board(&self, key: lpa_devices::BoardKey) -> Option<DeviceId> {
        self.devices
            .roster()
            .devices()
            .iter()
            .find(|device| {
                device
                    .identity
                    .mac
                    .as_ref()
                    .and_then(lpa_devices::BoardKey::from_mac)
                    == Some(key)
            })
            .map(|device| device.id)
            .or_else(|| {
                self.account_boards
                    .row(key)
                    .map(|_| crate::account_board_device(key))
            })
    }

    /// The board with MAC `key` by its card's name.
    pub(super) fn board_title(&self, key: lpa_devices::BoardKey) -> String {
        if let Some(device) = self.device_for_board(key)
            && let Some(device) = self.devices.roster().device(device)
        {
            return lpa_devices::view::device_view(device, self.device_now()).title;
        }
        self.account_boards
            .row(key)
            .map_or_else(|| key.to_string(), |row| row.label.clone())
    }

    /// The session's verbs on `view`'s card: `connect` on a board Studio is
    /// talking to ([`crate::device_connect_offer`]), `edit`
    /// ([`crate::device_edit_offer`]) and `done`
    /// ([`crate::device_done_offer`], on the board this tab's session is
    /// on, whoever holds it).
    pub(super) fn session_offers(
        &self,
        view: &crate::DeviceView,
        facts: &crate::DeviceOfferFacts<'_>,
        roster: &crate::DeviceRosterView,
    ) -> Vec<UiOffer> {
        let session_on_it = self
            .pool
            .attached_session()
            .is_some_and(|session| session.attachment().device == view.id);
        let uid = roster.open_addresses.get(&view.id.0).map(String::as_str);
        let connect = crate::ConnectFacts {
            registered: uid.is_some(),
            session_on_it,
            granted: self
                .devices
                .roster()
                .device(view.id)
                .is_some_and(|device| self.access.link_is_granted(device)),
            icon: crate::link_icon(roster.link_kinds.get(&view.id).copied().unwrap_or_default()),
            reach: self.connect_reach(view, facts),
            waiting: uid.is_some_and(|uid| {
                self.pending_device_lens
                    .as_ref()
                    .is_some_and(|pending| pending.is_connect_for(uid))
            }),
        };
        // Edit is not offered where the editor already shows the board:
        // there is nowhere further to go.
        let editor_shows_it = session_on_it && !self.connected_shows_on_its_card();
        let mut offers = Vec::new();
        offers.extend(crate::device_connect_offer(&facts.prefix, view, &connect));
        if !editor_shows_it {
            offers.extend(crate::device_edit_offer(&facts.prefix, view, uid));
        }
        if session_on_it {
            offers.push(crate::device_done_offer(&facts.prefix));
        }
        offers
    }

    /// How Connect reaches `view`'s board while it is offline: the first of
    /// its roads ([`Self::roads_to`]: Wi‑Fi, lightplayer.app, its cable).
    /// `None` for a board that is here, one nothing reaches, and a stand-in
    /// (a sim or an in-tab board, whose way back is Power on).
    fn connect_reach(
        &self,
        view: &crate::DeviceView,
        facts: &crate::DeviceOfferFacts<'_>,
    ) -> Option<crate::ConnectReach> {
        if view.status != crate::DeviceStatus::Offline || facts.face != crate::DeviceFace::Wire {
            return None;
        }
        // A board another tab holds over USB: its port is that tab's, and
        // the take-over is the way in (the "one tab" gate on the cable's
        // reach, as on a closed port's). The network roads carry their own
        // gate (the slot).
        let port_elsewhere = view
            .held_elsewhere
            .as_ref()
            .is_some_and(|held| held.via == lpa_devices::HoldVia::Usb);
        let cable = view.escapes.contains(&lpa_devices::view::Escape::Reconnect) && !port_elsewhere;
        match self.board_key(view.id) {
            Some(key) => self
                .roads_to(key, cable)
                .first()
                .map(crate::app::devices::best_link::Road::reach),
            None => cable.then_some(crate::ConnectReach::Usb),
        }
    }

    /// Keep why `device`'s connect did not open, for its card; hand the
    /// error on to the dispatch.
    fn connect_failed(&mut self, device: DeviceId, error: UiError) -> UiError {
        self.connect_failure = Some(self.connect_failure_for(device, &error));
        self.mark_dirty();
        error
    }

    /// `device`'s connect failure for `reason`, on the link the board is on
    /// now (none, when it was not reached).
    fn connect_failure_for(
        &self,
        device: DeviceId,
        reason: &dyn core::fmt::Display,
    ) -> ConnectFailure {
        ConnectFailure {
            device,
            reason: reason.to_string(),
            link: self
                .devices
                .roster()
                .device(device)
                .and_then(lpa_devices::Device::link),
        }
    }

    /// Let a failed Connect's words go once they say nothing true: the
    /// board is ready on another link than the one it failed on, reached
    /// by another road ([`ConnectFailure::outlived`]), or is forgotten.
    pub(super) fn settle_connect_failure(&mut self) {
        let Some(failure) = &self.connect_failure else {
            return;
        };
        let outlived = match self.devices.roster().device(failure.device) {
            None => true,
            Some(board) => {
                let status = lpa_devices::view::device_view(board, self.device_now()).status;
                let ready = matches!(
                    status,
                    lpa_devices::device::DeviceStatus::Ready
                        | lpa_devices::device::DeviceStatus::Degraded
                );
                failure.outlived(board.link(), ready)
            }
        };
        if outlived {
            self.connect_failure = None;
            self.mark_dirty();
        }
    }

    /// Let a failed Wi‑Fi or lightplayer.app connect's words go once a link
    /// of this tab reaches the board by any road: plugged in after it
    /// failed, the card would otherwise keep saying Studio couldn't reach it.
    pub(super) fn settle_wifi_connect_failures(&mut self) {
        let mut connects = core::mem::take(&mut self.wifi_connects);
        if connects.settle_reached(|key| self.reaches_board(key)) {
            self.mark_dirty();
        }
        self.wifi_connects = connects;
    }

    /// A held open (a project card or an address waiting for its board)
    /// is over when a Connect opens a session elsewhere: the lens would
    /// otherwise land with the open's project to send.
    fn let_a_held_open_go(&mut self) {
        if self.pending_open.take().is_some() {
            self.open_mismatch = None;
            crate::app::open_progress::note_open_settled();
        }
    }
}
