//! What core asks lightplayer.app to change on the account's board list,
//! and what it does with the answers ([`crate::AccountBoardRequest`]).
//!
//! - **The report** (plan D5): when a board's access list is read here —
//!   over USB, or over Bluetooth or the LAN once unlocked at edit — and it
//!   holds the signed-in account's **current** key, the board belongs on the
//!   account's list: core asks for it once per link per connection, with the
//!   board's own facts (its own name, never this browser's rename). The
//!   relay's link never asks: the relay writes its own.
//! - **Remove from your account** (P5): pressed, the board leaves the list
//!   here at once and the forget goes out; a failed forget brings it back,
//!   and its card says so.
//!
//! Core makes no call itself: the web edge takes each request
//! ([`StudioController::set_on_account_board_request`]) and hands the
//! answer back ([`crate::StudioCommand::AccountBoardAnswered`]).

use lpa_devices::BoardKey;
use lpa_devices::view::LoadedProject;

use super::StudioController;
use crate::app::devices::account_board_request::board_lan_label;
use crate::app::studio::studio_view_channel::CommandSender;
use crate::{AccountBoardAnswer, AccountBoardReport, AccountBoardRequest};

impl StudioController {
    /// Where the account board list's requests go (the web edge, which
    /// makes the cloud call), with the actor's command queue to hand the
    /// answer back on ([`crate::StudioCommand::AccountBoardAnswered`];
    /// `None` before the actor owns the controller). Install before the
    /// actor takes ownership.
    pub fn set_on_account_board_request(
        &mut self,
        hook: impl Fn(&AccountBoardRequest, Option<CommandSender>) + 'static,
    ) {
        self.on_account_board_request = Some(Box::new(hook));
    }

    /// Report every board the access controller found holding the
    /// account's current key since the last pass.
    pub(super) fn report_account_sightings(&mut self) {
        let sightings = self.access.take_account_sightings();
        if sightings.is_empty() || self.access.account_keys().is_none() {
            return;
        }
        let now = self.device_now();
        for sighting in sightings {
            let Some(device) = self.devices.roster().device(sighting.device) else {
                continue;
            };
            let Some(key) = device.identity.mac.as_ref().and_then(BoardKey::from_mac) else {
                continue;
            };
            let view = lpa_devices::view::device_view(device, now);
            let label = device
                .identity
                .name
                .clone()
                .filter(|name| !name.trim().is_empty())
                .unwrap_or_else(|| board_lan_label(key.octets()));
            let firmware = view
                .firmware_face
                .firmware()
                .map(str::to_string)
                .or(view.remembered_firmware.clone());
            let project = match view.loaded_project {
                LoadedProject::Running { label } => Some(label),
                LoadedProject::Empty | LoadedProject::Unknown => None,
            };
            self.send_account_board_request(AccountBoardRequest::Report(AccountBoardReport {
                id: key.to_string(),
                label,
                model: view.board_id.clone(),
                firmware,
                project,
                seen_on: sighting.link,
            }));
        }
    }

    /// `devices/<board>/forget-from-account` for `view`, at `prefix`: while
    /// someone is signed in (a guest holds no account key, so no list) and
    /// the account's list holds the board.
    pub(super) fn forget_from_account_offer_for(
        &self,
        view: &crate::DeviceView,
        prefix: &crate::OfferPath,
    ) -> Option<crate::UiOffer> {
        self.access.account_keys()?;
        let key = self.board_key(view.id)?;
        self.account_boards
            .listed(key)
            .then(|| crate::forget_from_account_offer(prefix, key, &view.title))
    }

    /// Remove from your account, pressed (and armed): the board leaves the
    /// list here at once, and the forget goes to lightplayer.app. Its answer
    /// settles it ([`Self::account_board_answered`]).
    pub(super) fn forget_from_account(&mut self, op: crate::ForgetFromAccountOp) {
        if self.account_boards.forget(op.board) {
            self.account_boards.clear_note(op.board);
            self.send_account_board_request(AccountBoardRequest::Forget {
                id: op.board.to_string(),
            });
        }
        self.mark_dirty();
    }

    /// lightplayer.app answered a request: a full list or a failed forget
    /// is said on the board's card; a forget's answer settles the row.
    pub fn account_board_answered(&mut self, answer: AccountBoardAnswer) {
        let key_of = |id: &str| BoardKey::parse(id).ok();
        match answer {
            AccountBoardAnswer::Reported { id } => {
                if let Some(key) = key_of(&id) {
                    self.account_boards.clear_note(key);
                }
            }
            AccountBoardAnswer::ListFull { id, max } => {
                if let Some(key) = key_of(&id) {
                    self.account_boards
                        .note(key, crate::AccountBoardNote::ListFull { max });
                }
            }
            AccountBoardAnswer::Forgotten { id } => {
                if let Some(key) = key_of(&id) {
                    self.account_boards.forget_answered(key, true);
                    self.account_boards.clear_note(key);
                }
            }
            AccountBoardAnswer::Failed { request, error } => match request {
                AccountBoardRequest::Forget { id } => {
                    if let Some(key) = key_of(&id) {
                        self.account_boards.forget_answered(key, false);
                        self.account_boards
                            .note(key, crate::AccountBoardNote::ForgetFailed(error));
                    }
                }
                // A report that did not land is tried again on the board's
                // next connection; nothing to say on its card.
                AccountBoardRequest::Report(report) => {
                    log::info!(
                        "account boards: reporting {} didn't land: {error}",
                        report.id
                    );
                }
            },
        }
        self.mark_dirty();
    }

    /// Hand one request to the web edge.
    pub(super) fn send_account_board_request(&self, request: AccountBoardRequest) {
        match &self.on_account_board_request {
            Some(hook) => hook(&request, self.wifi_tx.clone()),
            None => log::debug!("account boards: no edge to send {request:?}"),
        }
    }
}
