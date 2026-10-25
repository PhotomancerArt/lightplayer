//! [`UiCloudBoard`]: a board on the signed-in account's list, as its card
//! reads it — the account's row for it (`ListAccountBoards`), online at
//! lightplayer.app now or not, and its last picture (`BoardPictures`).
//! Joined by the controller onto the roster view
//! (`DeviceRosterView::cloud_boards`), by MAC, for a board this browser
//! knows and for one it never met alike ([`super::account_boards`]).

use lpa_devices::BoardKey;

use crate::UiControlProductPreview;

/// A listed board's state while no link of this tab reaches it: the
/// roster's word for it (the details' "State", the home page's row).
pub const ONLINE_AT_LIGHTPLAYER_APP: &str = "Online at lightplayer.app";

/// See the module docs.
#[derive(Clone, Debug, PartialEq)]
pub struct UiCloudBoard {
    /// The board's MAC: its relay id.
    pub key: BoardKey,
    /// The board's name, as it last reported it.
    pub label: String,
    /// Its catalog model (a board id), once a Studio reported one.
    pub model: Option<String>,
    /// The firmware version last reported.
    pub firmware: Option<String>,
    /// The project it plays, by name, as it last reported it.
    pub project: Option<String>,
    /// Online at lightplayer.app now. Offline, the card is an offline
    /// board's: its facts and its picture the last known, dimmed.
    pub online: bool,
    /// When any link last saw it, epoch seconds.
    pub last_seen: Option<f64>,
    /// The relay protocol it speaks while online (1 sends no picture); 0
    /// while it is offline.
    pub relay_proto: u16,
    /// Its last picture, drawable; `None` before one arrives, and always on
    /// relay protocol 1 (the card says nothing about that).
    pub picture: Option<UiControlProductPreview>,
    /// When that picture arrived at lightplayer.app, epoch seconds.
    pub picture_at: Option<f64>,
}

/// One board whose card draws lightplayer.app's picture — an account board
/// no link of this tab reaches — and the picture it already holds: what the
/// web edge asks `BoardPictures` for, watching
/// (`DeviceRosterView::cloud_picture_asks`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CloudPictureAsk {
    /// The board's relay id.
    pub id: String,
    /// The picture it holds, if any: an unchanged one comes back without
    /// its colours.
    pub seq: Option<u64>,
}
