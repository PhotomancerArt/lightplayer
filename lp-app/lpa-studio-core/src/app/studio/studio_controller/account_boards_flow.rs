//! The account's boards on every computer: the rows lightplayer.app's relay
//! lists (`ListBoards`, [`crate::AccountBoards`]) joined onto the roster,
//! by MAC.
//!
//! - **A board this browser knows** (remembered or live): it is "online at
//!   lightplayer.app". With no link here its card moves to Online boards and
//!   says so, and Connect through lightplayer.app is its way in; with a USB,
//!   Bluetooth or LAN link it keeps that link (plan D7), and the row only
//!   tells the lightplayer.app row what it already says.
//! - **A board it never met** is an in-memory roster entry under its MAC
//!   (`mac-<12 hex>`, handle [`crate::account_board_device`]), named by the
//!   row. It is never written to `/registry.json` (plan D6): once it says
//!   hello over a link it is a stranger the roster remembers as usual, and
//!   the entry folds into that board — while its link is still identifying,
//!   the pending card stands for it alone.
//! - **A board that leaves the list** goes back to what this browser knew
//!   of it, or away.

use lpa_devices::BoardKey;
use lpa_devices::device::DeviceStatus;
use lpa_devices::view::{DeviceView, FirmwareFace, LoadedProject};

use super::StudioController;
use crate::app::devices::ui_cloud_board::ONLINE_AT_LIGHTPLAYER_APP;

impl StudioController {
    /// Join the account's rows onto `view`: each listed board's
    /// [`crate::UiCloudBoard`], an entry for each one the roster does not
    /// hold, and the pictures the home page asks lightplayer.app for.
    pub(super) fn join_account_boards(&self, view: &mut crate::DeviceRosterView) {
        let pending: Vec<BoardKey> = self
            .devices
            .roster()
            .pending()
            .iter()
            .filter_map(|pending| relay_board(&pending.info.endpoint.0))
            .collect();
        for (key, _) in self.account_boards.rows() {
            let Some(cloud) = self.account_boards.view(key) else {
                continue;
            };
            let known = view
                .roster
                .devices
                .iter_mut()
                .find(|device| self.board_key(device.id) == Some(key));
            let device = match known {
                Some(device) => {
                    if device.status == DeviceStatus::Offline && device.held_elsewhere.is_none() {
                        device.state_label = ONLINE_AT_LIGHTPLAYER_APP.to_string();
                    }
                    device.id
                }
                // Its relay link is identifying: the pending card is the
                // board's, until its hello says who it is.
                None if pending.contains(&key) => continue,
                None => {
                    let entry = account_board_view(&cloud);
                    let device = entry.id;
                    view.link_kinds.insert(device, crate::UiLinkKind::Relay);
                    view.roster.devices.push(entry);
                    device
                }
            };
            if !self.reaches_board(key) {
                view.cloud_picture_asks.push(crate::CloudPictureAsk {
                    id: key.to_string(),
                    seq: self.account_boards.picture_seq(key),
                });
            }
            view.cloud_boards.insert(device, cloud);
        }
    }

    /// Whether a link of this tab reaches the board with MAC `key` right
    /// now (USB, Bluetooth, the LAN or the relay).
    pub(super) fn reaches_board(&self, key: BoardKey) -> bool {
        self.devices.roster().devices().iter().any(|device| {
            device.status() != DeviceStatus::Offline
                && device.identity.mac.as_ref().and_then(BoardKey::from_mac) == Some(key)
        })
    }

    /// The board an account entry's handle names, while it is listed and
    /// the roster holds no board by its MAC.
    pub(super) fn account_board_key(&self, device: crate::DeviceId) -> Option<BoardKey> {
        self.account_boards.key_of(device)
    }

    /// Whether `device` is an account entry: a board this browser never
    /// met, known only by lightplayer.app's row.
    pub(super) fn is_account_entry(&self, device: crate::DeviceId) -> bool {
        self.devices.roster().device(device).is_none() && self.account_board_key(device).is_some()
    }
}

/// The board a `relay:<mac>` endpoint names.
fn relay_board(endpoint: &str) -> Option<BoardKey> {
    lpa_link::providers::network_link::board_from_relay_endpoint(endpoint)
        .and_then(|board| BoardKey::parse(board).ok())
}

/// The roster entry for an account board this browser never met: offline
/// here, named by the row, its firmware and project as the board last told
/// lightplayer.app.
fn account_board_view(cloud: &crate::UiCloudBoard) -> DeviceView {
    DeviceView {
        id: crate::account_board_device(cloud.key),
        title: cloud.label.clone(),
        status: DeviceStatus::Offline,
        state_label: ONLINE_AT_LIGHTPLAYER_APP.to_string(),
        detail: None,
        freshness_label: None,
        heard_age_ms: None,
        identity_label: None,
        detected_chip: None,
        board_id: None,
        firmware_face: FirmwareFace::Unknown,
        remembered_firmware: cloud.firmware.clone(),
        degraded: None,
        loaded_project: match &cloud.project {
            Some(label) => LoadedProject::Running {
                label: label.clone(),
            },
            None => LoadedProject::Unknown,
        },
        engine_fps: None,
        link_counters: None,
        can_receive_project: false,
        can_remove_project: false,
        activity: None,
        last_outcome: None,
        last_update_outcome: None,
        terminal: Vec::new(),
        terminal_dropped: 0,
        firmware_blocked: None,
        update_blocked: None,
        held_elsewhere: None,
        escapes: Vec::new(),
    }
}
