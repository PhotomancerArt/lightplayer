//! The move of a board's session from lightplayer.app onto Wi‑Fi, the
//! controller's half (the machine is `devices/lan_move.rs`).
//!
//! From the refresh tick ([`StudioController::drive_lan_moves`]): a board
//! reached through lightplayer.app, idle, whose account list says it shares
//! this network at an address, on a page that may reach the LAN, moves there
//! once per page load:
//!
//! 1. its LAN link will present the relay's keys first
//!    ([`crate::NetworkLinkKeys::present_as_relay`]), so the board hands its
//!    one network slot over instead of answering busy;
//! 2. its relay session is parked ([`crate::RelayDeviceTransport::park`]):
//!    when the board closes it for the LAN connection, it is not redialled
//!    to take the slot back;
//! 3. the LAN connection is dialled; its answer is the move's, not the
//!    card's (no "Connecting…" and no failure words on the card);
//! 4. once the board's own link is the LAN's, the editor (when it is on the
//!    board) is moved onto it in one step, never held, and the relay session
//!    is let go. On any failure the relay session is unparked and the board
//!    stays, with one line in the log.
//!
//! A moved board whose Wi‑Fi link goes is held by the editor as any dropped
//! link is; if Wi‑Fi is not back within five seconds the page goes back
//! through lightplayer.app — unless the link was closed for another
//! connection (code 1000), or the list no longer names the board.

use lpa_devices::BoardKey;
use lpa_devices::device::DeviceStatus;
use lpa_link::providers::network_link::{
    board_from_relay_endpoint, lan_host, url_from_lan_endpoint,
};

use super::StudioController;
use crate::app::devices::lan_move::{LanMoveEnd, LanMoveLook, LanMoveStep};
use crate::{UiLogDraft, UiLogLevel, UiLogOrigin};

impl StudioController {
    /// Look at every board's move once (from the refresh tick): start the
    /// one a board is ready for, and take each move under way a step.
    pub(crate) async fn drive_lan_moves(&mut self) {
        if let Some((key, url)) = self.lan_move_candidate() {
            self.start_lan_move(key, &url);
        }
        let now = (self.now_secs)();
        for key in self.lan_moves.active() {
            let look = self.lan_move_look(key);
            match self.lan_moves.look(key, look, now) {
                LanMoveStep::Wait | LanMoveStep::Over => {}
                LanMoveStep::Bound { url } => self.finish_lan_move(key, &url).await,
                LanMoveStep::GaveUp { url } => {
                    self.forget_lan_session(&url);
                    self.stay_on_relay(
                        key,
                        "the board didn't answer over Wi\u{2011}Fi in time".to_string(),
                    );
                }
                LanMoveStep::Lost => {
                    log::info!("wi-fi: {key}: the moved link went; waiting for it to come back");
                }
                LanMoveStep::Back => {
                    log::info!("wi-fi: {key}: back on Wi-Fi");
                }
                LanMoveStep::Return { url } => self.return_to_relay(key, &url),
                LanMoveStep::NoReturn { url, why } => self.no_return(key, &url, why),
            }
            self.mark_dirty();
        }
    }

    /// The move's LAN dial for `board` answered: on to waiting for the
    /// board's own link, or stay on lightplayer.app. `false` when `board`
    /// had no move dialling (the answer is a card's).
    pub(super) fn lan_move_dialled(
        &mut self,
        board: BoardKey,
        result: Result<(), crate::WifiConnectFailure>,
    ) -> bool {
        if !self.lan_moves.dialling(board) {
            return false;
        }
        let now = (self.now_secs)();
        match result {
            Ok(()) => {
                self.lan_moves.dialled(board, true, now);
                self.device_sweep_pending = true;
                self.run_due_device_sweep();
            }
            Err(failure) => {
                self.lan_moves.dialled(board, false, now);
                self.stay_on_relay(board, failure.words());
            }
        }
        self.mark_dirty();
        true
    }

    /// A link error was heard on `link` (`message`): a moved board's Wi‑Fi
    /// link closed with code 1000 was handed to another connection.
    pub(super) fn note_lan_move_link_error(&mut self, link: lpa_devices::LinkId, message: &str) {
        if !crate::app::devices::wifi_connect_failure::closed_for_another(message) {
            return;
        }
        let Some(url) = self
            .devices
            .roster()
            .link_info(link)
            .and_then(|info| url_from_lan_endpoint(&info.endpoint.0).map(str::to_string))
        else {
            return;
        };
        let moved: Vec<BoardKey> = self
            .lan_moves
            .active()
            .into_iter()
            .filter(|key| self.lan_moves.url(*key) == Some(url.as_str()))
            .collect();
        for key in moved {
            self.lan_moves.note_taken(key);
        }
    }

    /// The board ready to move now, with the LAN address to dial: reached
    /// through lightplayer.app, ready and holding a tier, idle, on the
    /// account's list as sharing this network at an address, never tried
    /// this page load — and nothing else under way for it here.
    fn lan_move_candidate(&self) -> Option<(BoardKey, String)> {
        if self.lan_transport.is_none()
            || self.relay_transport.is_none()
            || self.access.account_keys().is_none()
            || !self.lan_reach.tries_by_itself()
            || self.pending_open.is_some()
            || self.pending_device_lens.is_some()
            || self.lens_hold.is_some()
        {
            return None;
        }
        let now = self.device_now();
        self.devices.roster().devices().iter().find_map(|device| {
            let key = device.identity.mac.as_ref().and_then(BoardKey::from_mac)?;
            let endpoint = device.identity.endpoint.as_ref()?;
            board_from_relay_endpoint(&endpoint.0)?;
            if self.lan_moves.tried(key)
                || self.fall_through.contains(&key)
                || self.network_slot_held_elsewhere(key)
                || self.take_overs.devices().any(|id| id == device.id)
                || self
                    .wifi_connects
                    .connecting(crate::WifiConnectTarget::Board(key))
                || self
                    .wifi_connects
                    .connecting(crate::WifiConnectTarget::Relay(key))
                || self.devices.effects().update_host().is_running(device.id)
                || !self.access.link_is_granted(device)
            {
                return None;
            }
            let view = lpa_devices::view::device_view(device, now);
            let ready = matches!(view.status, DeviceStatus::Ready | DeviceStatus::Degraded);
            if !ready || view.activity.is_some() || view.held_elsewhere.is_some() {
                return None;
            }
            let hint = self
                .account_lan_hint(key)
                .filter(|hint| hint.same_network)?;
            let url = crate::normalize_lan_address(hint.lan.as_deref()?).ok()?;
            url.starts_with("ws://").then_some((key, url))
        })
    }

    /// Start `key`'s move: the relay's keys first on its LAN link, its relay
    /// session parked, and the LAN connection to `url` dialled.
    fn start_lan_move(&mut self, key: BoardKey, url: &str) {
        let (Some(lan), Some(relay), Some(spawner), Some(tx)) = (
            self.lan_transport.clone(),
            self.relay_transport.clone(),
            self.wifi_spawner.clone(),
            self.wifi_tx.clone(),
        ) else {
            return;
        };
        let board = key.to_string();
        let keys = self.access.network_link_keys();
        keys.alias(url, &board);
        keys.present_as_relay(&board, true);
        relay.park(&board);
        self.lan_moves.start(key, url);
        log::info!("wi-fi: {board}: moving its session from lightplayer.app onto {url}");
        let host = lan_host(url).to_string();
        let attempt = lan.connect(url);
        let target = crate::WifiConnectTarget::Board(key);
        spawner(Box::pin(async move {
            let result = attempt.await;
            tx.send(crate::StudioCommand::Network(
                crate::app::network::NetworkCommand::WifiConnected {
                    target,
                    host,
                    result,
                },
            ));
        }));
        self.mark_dirty();
    }

    /// What `key`'s board looks like to its move now.
    fn lan_move_look(&self, key: BoardKey) -> LanMoveLook {
        let listed = self.account_lan_hint(key).is_some();
        let Some(url) = self.lan_moves.url(key) else {
            return LanMoveLook {
                listed,
                ..LanMoveLook::default()
            };
        };
        let now = self.device_now();
        let mut look = LanMoveLook {
            listed,
            ..LanMoveLook::default()
        };
        for device in self.devices.roster().devices() {
            if device.identity.mac.as_ref().and_then(BoardKey::from_mac) != Some(key) {
                continue;
            }
            let view = lpa_devices::view::device_view(device, now);
            if !matches!(view.status, DeviceStatus::Ready | DeviceStatus::Degraded) {
                continue;
            }
            let endpoint = device
                .identity
                .endpoint
                .as_ref()
                .map(|endpoint| endpoint.0.as_str());
            match endpoint {
                Some(endpoint) if url_from_lan_endpoint(endpoint) == Some(url) => {
                    look.on_lan |= self.access.link_is_granted(device);
                }
                Some(endpoint) if board_from_relay_endpoint(endpoint).is_some() => {}
                Some(endpoint) if url_from_lan_endpoint(endpoint).is_some() => {}
                Some(_) => look.reached_otherwise = true,
                None => {}
            }
        }
        look
    }

    /// `key`'s board is on Wi‑Fi now: move the editor onto the new link
    /// when it is on this board (in one step, with no "Reconnecting…"), let
    /// the relay session go, and say so once.
    async fn finish_lan_move(&mut self, key: BoardKey, url: &str) {
        let board = key.to_string();
        let device = self.device_for_board(key);
        let on_this_board = self
            .pool
            .attached_session()
            .filter(|session| Some(session.attachment().device) == device)
            .map(|session| session.attachment().clone());
        if let Some(attachment) = on_this_board
            && let Err(error) = self.move_lens_onto_current_link(&attachment).await
        {
            // The relay's wire is let go already: the editor waits for the
            // board the way it waits across any dropped link, on whichever
            // link it holds now.
            log::info!("wi-fi: {board}: the editor did not move onto Wi-Fi: {error}");
            let holding = self
                .pool
                .attached_session()
                .map(|session| session.attachment().clone())
                .unwrap_or(attachment);
            self.hold_device_lens(&holding);
        }
        self.access
            .network_link_keys()
            .present_as_relay(&board, false);
        if let (Some(relay), Some(spawner)) =
            (self.relay_transport.clone(), self.wifi_spawner.clone())
        {
            let forget = relay.forget_session(&board);
            spawner(Box::pin(async move {
                if let Err(error) = forget.await {
                    log::warn!("relay: letting the session go failed: {error}");
                }
            }));
        }
        self.push_log(UiLogDraft::new(
            UiLogLevel::Info,
            UiLogOrigin::Studio,
            "Moved to Wi\u{2011}Fi on your network".to_string(),
        ));
        self.record_device_event(
            None,
            None,
            crate::core::log::DeviceEventKind::Pool {
                action: "move".to_string(),
                detail: format!("{board} moved from lightplayer.app onto {url}"),
            },
        );
    }

    /// The move of `key` is over and the board stays on lightplayer.app
    /// (`why`, in plain words): its relay session is unparked (redialled at
    /// once if the board closed it meanwhile), and the log says so once.
    fn stay_on_relay(&mut self, key: BoardKey, why: String) {
        let board = key.to_string();
        if let Some(relay) = &self.relay_transport {
            relay.unpark(&board);
        }
        self.access
            .network_link_keys()
            .present_as_relay(&board, false);
        let why = why.trim_end_matches('.');
        self.push_log(UiLogDraft::new(
            UiLogLevel::Info,
            UiLogOrigin::Studio,
            format!("Staying on lightplayer.app: {why}"),
        ));
    }

    /// Wi‑Fi did not come back in time: let the LAN session go (it stops
    /// redialling) and reach the board through lightplayer.app again.
    fn return_to_relay(&mut self, key: BoardKey, url: &str) {
        self.forget_lan_session(url);
        let started = self
            .device_for_board(key)
            .map(|device| self.start_relay_connect(crate::RelayConnectOp { device }));
        if let Some(Err(error)) = started {
            log::warn!("relay: {key}: going back through lightplayer.app failed: {error}");
        }
        self.push_log(UiLogDraft::new(
            UiLogLevel::Info,
            UiLogOrigin::Studio,
            "Wi\u{2011}Fi dropped; back through lightplayer.app".to_string(),
        ));
        self.record_device_event(
            None,
            None,
            crate::core::log::DeviceEventKind::Pool {
                action: "move".to_string(),
                detail: format!("{key} went back through lightplayer.app from {url}"),
            },
        );
    }

    /// Wi‑Fi went and the page does not go back (`why`): another connection
    /// took the board's slot (the card says someone else is connected, and
    /// the LAN session stops redialling, which would only take it back), or
    /// the list no longer names the board.
    fn no_return(&mut self, key: BoardKey, url: &str, why: LanMoveEnd) {
        log::info!("wi-fi: {key}: not going back through lightplayer.app ({why:?})");
        if why == LanMoveEnd::Taken {
            self.forget_lan_session(url);
            self.wifi_connects.finish(
                crate::WifiConnectTarget::Board(key),
                lan_host(url),
                Err(crate::WifiConnectFailure::Busy),
            );
        }
    }

    /// Let the LAN session at `url` go by request.
    fn forget_lan_session(&self, url: &str) {
        if let (Some(lan), Some(spawner)) = (self.lan_transport.clone(), self.wifi_spawner.clone())
        {
            let forget = lan.forget_session(url);
            spawner(Box::pin(async move {
                if let Err(error) = forget.await {
                    log::warn!("wi-fi: letting the session go failed: {error}");
                }
            }));
        }
    }
}

#[cfg(test)]
impl StudioController {
    /// Where `key`'s move stands, for the tests.
    pub(crate) fn lan_moves_stage_for_test(
        &self,
        key: BoardKey,
    ) -> Option<crate::app::devices::lan_move::LanMoveStage> {
        self.lan_moves.stage(key).cloned()
    }
}
