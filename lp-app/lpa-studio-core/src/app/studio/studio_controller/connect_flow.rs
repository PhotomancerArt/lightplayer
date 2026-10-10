//! Connected: the home page holds the open session.
//!
//! Connect (`devices/<board>/connect`), Edit (`devices/<board>/edit`) and
//! Done (`devices/<board>/done`) on the studio controller, and the one rule
//! for which surface an open session shows on
//! ([`crate::ConnectedBoard::shows_editor`]). The session itself is the
//! editor's lens, opened the way an address opens it
//! ([`StudioController::open_device_lens_for`]): this file records who holds
//! it, and nothing about the runtime pool changes.

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

    /// The web reported `place`: an Edit waiting for the editor ends once
    /// the user is on the session's page, where the place itself shows it.
    /// Read-only, like every use of the place: nothing opens or closes.
    pub(super) fn note_place_for_connected(&mut self, place: &crate::UiPlace) {
        let lens_project = self.project.active_library_uid();
        if let Some(connected) = self.connected_mut()
            && connected.editor_waiting
            && connected.is_its_page(&place.page, lens_project.as_deref())
        {
            connected.editor_waiting = false;
        }
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
    /// `editor_waiting`): open `device`'s session held by the home page.
    ///
    /// - Already connected here: a Connect has nothing to do; an Edit shows
    ///   the editor on the session open now, reattaching and reading
    ///   nothing.
    /// - A session an address opened on this board: the home page takes it
    ///   over as it is, with nothing reopened.
    /// - Otherwise the lens opens the way an address opens it, a session on
    ///   another board closing first (one board connected at a time), and a
    ///   held open on another board is let go — a Connect never sends a
    ///   project anywhere.
    ///
    /// A connect that cannot open leaves its reason for the board's card.
    pub(super) async fn connect_device(
        &mut self,
        device: DeviceId,
        editor_waiting: bool,
        updates: UxUpdateSink,
    ) -> UiResult {
        self.connect_failure = None;
        if let Some(connected) = self.connected_mut()
            && connected.device == device
        {
            connected.editor_waiting |= editor_waiting;
            self.mark_dirty();
            return Ok(UiNotices::new());
        }
        let Some(uid) = self.devices.key_for(device).map(str::to_string) else {
            return Err(self.connect_failed(
                device,
                UiError::UnsupportedAction("this board has not said who it is yet".to_string()),
            ));
        };
        if let Some(id) = self
            .pool
            .attached_session()
            .filter(|session| session.attachment().device == device)
            .map(crate::RuntimeSession::id)
        {
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
        self.let_a_held_open_go();
        let intent = ConnectIntent { editor_waiting };
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
        self.connect_device(device, true, updates).await
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

    /// Keep why `device`'s connect did not open, for its card; hand the
    /// error on to the dispatch.
    fn connect_failed(&mut self, device: DeviceId, error: UiError) -> UiError {
        self.connect_failure = Some(ConnectFailure {
            device,
            reason: error.to_string(),
        });
        self.mark_dirty();
        error
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
