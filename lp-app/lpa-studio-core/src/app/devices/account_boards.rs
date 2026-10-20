//! The signed-in account's boards that are online at lightplayer.app right
//! now, as its relay reports them (`ListBoards`), and their last pictures
//! (`BoardPictures`).
//!
//! Studio core does not know the cloud. The web edge asks
//! (`lpa-studio-web/src/cloud/account_boards.rs`) and hands the answers in
//! as [`crate::StudioCommand::AccountBoards`] and
//! [`crate::StudioCommand::AccountBoardPictures`], stamped with its own
//! clock; this holds the latest, by MAC. Sans-IO, no clock.
//!
//! Nothing here is persisted (plan D6): a board this browser never met is an
//! in-memory roster entry under its MAC ([`account_board_device`]) until it
//! says hello over a link and is remembered as usual. Signing out, or a
//! lightplayer.app that does not answer, keeps nothing: a board that leaves
//! the list goes back to what this browser knew of it, or goes away.

use std::collections::BTreeMap;

use lpa_devices::{BoardKey, DeviceId};

use super::cloud_board_picture::{CloudBoardPicture, cloud_picture_preview};
use super::ui_cloud_board::UiCloudBoard;

/// One board of the account online at lightplayer.app, as `ListBoards`
/// reports it (`lpc_cloud_api::BoardPresence`, carried over by the web).
#[derive(Clone, Debug, PartialEq)]
pub struct AccountBoardRow {
    /// The board's relay id: its MAC as twelve hex digits (`10bda3b08e30`).
    pub id: String,
    /// The board's name, as it reported it.
    pub label: String,
    /// The device wire version the board speaks.
    pub wire_proto: u32,
    /// Where the board says it answers on its own network
    /// (`192.168.4.20:80`), unchecked.
    pub lan: Option<String>,
    /// The caller and the board reach lightplayer.app from the same public
    /// address: a session may try the LAN (the automatic move is M3's).
    pub same_network: bool,
    /// When the board registered, epoch seconds.
    pub since: f64,
    /// The relay protocol the board speaks: 1 sends no picture, no firmware
    /// and no project.
    pub relay_proto: u16,
    /// The firmware version from the board's hello (protocol 2).
    pub firmware: Option<String>,
    /// The name of the project the board plays, as it last reported it
    /// (protocol 2).
    pub project: Option<String>,
}

/// A `ListBoards` answer, stamped with the caller's clock.
#[derive(Clone, Debug, PartialEq)]
pub struct AccountBoardRows {
    /// When the answer arrived, epoch seconds.
    pub at: f64,
    /// The account's online boards.
    pub boards: Vec<AccountBoardRow>,
}

/// One board's last picture, as `BoardPictures` reports it
/// (`lpc_cloud_api::BoardPicture`, carried over by the web).
#[derive(Clone, Debug, PartialEq)]
pub struct AccountBoardPicture {
    /// The board's relay id.
    pub id: String,
    /// Moves on every picture the relay accepts from the board.
    pub seq: u64,
    /// When the picture arrived at the relay, epoch seconds.
    pub at: f64,
    /// Lamps per output, in the project's tree order.
    pub outputs: Vec<u32>,
    /// R, G, B per sample, sRGB codes; `None` when the asker already held
    /// picture `seq` (the colours are the ones it has).
    pub colors: Option<Vec<u8>>,
}

/// Where a board this browser never met lives in the roster: a handle
/// derived from its MAC, far above anything the roster mints (it never
/// mints past `u32::MAX`) and below the legacy ids an old `/registry.json`
/// may hold (near `u64::MAX / 2`). The same board keeps the same handle
/// across every answer, so an offer built on one view still names it when
/// it is pressed on the next.
pub fn account_board_device(key: BoardKey) -> DeviceId {
    let mac = key
        .octets()
        .iter()
        .fold(0u64, |value, octet| (value << 8) | u64::from(*octet));
    DeviceId(ACCOUNT_BOARD_HANDLES | mac)
}

/// The bit every [`account_board_device`] handle carries.
const ACCOUNT_BOARD_HANDLES: u64 = 1 << 56;

/// The account's online boards and their pictures. See the module docs.
#[derive(Clone, Debug, Default)]
pub struct AccountBoards {
    rows: BTreeMap<BoardKey, AccountBoardRow>,
    pictures: BTreeMap<BoardKey, CloudBoardPicture>,
}

impl AccountBoards {
    /// The latest `ListBoards` answer replaces the last one; `None` (signed
    /// out, or lightplayer.app did not answer) keeps nothing. A row whose id
    /// is not a MAC is dropped. A picture outlives a row only while its
    /// board is listed.
    pub fn replace(&mut self, rows: Option<AccountBoardRows>) {
        self.rows = rows
            .into_iter()
            .flat_map(|rows| rows.boards)
            .filter_map(|row| Some((board_key(&row.id)?, row)))
            .collect();
        self.pictures.retain(|key, _| self.rows.contains_key(key));
    }

    /// A `BoardPictures` answer: each picture of a listed board replaces
    /// its last one. One that repeats the `seq` already held, without its
    /// colours, changes nothing. Whether any picture changed.
    pub fn take_pictures(&mut self, pictures: Vec<AccountBoardPicture>) -> bool {
        let mut changed = false;
        for picture in pictures {
            let Some(key) = board_key(&picture.id).filter(|key| self.rows.contains_key(key)) else {
                continue;
            };
            let Some(colors) = picture.colors else {
                continue;
            };
            self.pictures.insert(
                key,
                CloudBoardPicture {
                    seq: picture.seq,
                    at: picture.at,
                    outputs: picture.outputs,
                    colors,
                },
            );
            changed = true;
        }
        changed
    }

    /// Whether `key` is online at lightplayer.app, by the last answer.
    pub fn row(&self, key: BoardKey) -> Option<&AccountBoardRow> {
        self.rows.get(&key)
    }

    /// The listed boards, by MAC.
    pub fn rows(&self) -> impl Iterator<Item = (BoardKey, &AccountBoardRow)> {
        self.rows.iter().map(|(key, row)| (*key, row))
    }

    /// The board an [`account_board_device`] handle names, while it is
    /// listed.
    pub fn key_of(&self, device: DeviceId) -> Option<BoardKey> {
        self.rows
            .keys()
            .copied()
            .find(|key| account_board_device(*key) == device)
    }

    /// The picture `seq` held for `key`: what the next `BoardPictures` asks
    /// with, so an unchanged picture comes back without its colours.
    pub fn picture_seq(&self, key: BoardKey) -> Option<u64> {
        self.pictures.get(&key).map(|picture| picture.seq)
    }

    /// What a card says of `key` while it is listed: the row's facts and
    /// its picture, drawable.
    pub fn view(&self, key: BoardKey) -> Option<UiCloudBoard> {
        let row = self.rows.get(&key)?;
        let picture = self.pictures.get(&key);
        Some(UiCloudBoard {
            key,
            label: row.label.clone(),
            firmware: row.firmware.clone(),
            project: row.project.clone(),
            relay_proto: row.relay_proto,
            picture: picture.and_then(cloud_picture_preview),
            picture_at: picture.map(|picture| picture.at),
        })
    }
}

/// The board a relay id names; `None` for one that is not a MAC.
fn board_key(id: &str) -> Option<BoardKey> {
    let id: lpc_relay::RelayBoardId = id.parse().ok()?;
    BoardKey::from_octets(id.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_replace_rows() {
        let mut boards = AccountBoards::default();
        boards.replace(Some(rows(&["10bda3b08e30", "020000000001"])));
        assert_eq!(boards.rows().count(), 2);
        boards.replace(Some(rows(&["020000000001"])));
        let keys: Vec<String> = boards.rows().map(|(key, _)| key.to_string()).collect();
        assert_eq!(keys, ["020000000001"]);
    }

    #[test]
    fn signing_out_keeps_nothing() {
        let mut boards = AccountBoards::default();
        boards.replace(Some(rows(&["10bda3b08e30"])));
        boards.take_pictures(vec![picture("10bda3b08e30", 3, Some(vec![255, 0, 0]))]);
        boards.replace(None);
        assert_eq!(boards.rows().count(), 0);
        assert_eq!(boards.picture_seq(key("10bda3b08e30")), None);
    }

    #[test]
    fn a_row_without_a_mac_is_dropped() {
        let mut boards = AccountBoards::default();
        boards.replace(Some(rows(&[
            "not-a-mac",
            "10bd",
            "000000000000",
            "10:BD:A3:B0:8E:30",
        ])));
        let keys: Vec<String> = boards.rows().map(|(key, _)| key.to_string()).collect();
        assert_eq!(keys, ["10bda3b08e30"]);
    }

    #[test]
    fn a_picture_lands_on_a_listed_board_and_an_unchanged_one_changes_nothing() {
        let mut boards = AccountBoards::default();
        boards.replace(Some(rows(&["10bda3b08e30"])));
        boards.take_pictures(vec![
            picture("10bda3b08e30", 3, Some(vec![255, 0, 0])),
            picture("020000000001", 4, Some(vec![0, 255, 0])),
        ]);
        let board = key("10bda3b08e30");
        assert_eq!(boards.picture_seq(board), Some(3));
        assert_eq!(boards.picture_seq(key("020000000001")), None);
        boards.take_pictures(vec![picture("10bda3b08e30", 3, None)]);
        assert_eq!(boards.picture_seq(board), Some(3));
        assert!(boards.view(board).unwrap().picture.is_some());
    }

    #[test]
    fn a_boards_handle_is_its_mac_and_names_it_back() {
        let mut boards = AccountBoards::default();
        boards.replace(Some(rows(&["10bda3b08e30"])));
        let board = key("10bda3b08e30");
        let device = account_board_device(board);
        assert_eq!(device, DeviceId((1 << 56) | 0x10bd_a3b0_8e30));
        assert!(device.0 > u64::from(u32::MAX) && device.0 < u64::MAX / 4);
        assert_eq!(boards.key_of(device), Some(board));
        assert_eq!(boards.key_of(DeviceId(7)), None);
    }

    fn key(id: &str) -> BoardKey {
        board_key(id).unwrap()
    }

    fn rows(ids: &[&str]) -> AccountBoardRows {
        AccountBoardRows {
            at: 100.0,
            boards: ids
                .iter()
                .map(|id| AccountBoardRow {
                    id: id.to_string(),
                    label: "Fabric Test".to_string(),
                    wire_proto: 42,
                    lan: None,
                    same_network: false,
                    since: 50.0,
                    relay_proto: 2,
                    firmware: Some("2026.10.19-1".to_string()),
                    project: Some("rainbow".to_string()),
                })
                .collect(),
        }
    }

    fn picture(id: &str, seq: u64, colors: Option<Vec<u8>>) -> AccountBoardPicture {
        AccountBoardPicture {
            id: id.to_string(),
            seq,
            at: 99.0,
            outputs: vec![1],
            colors,
        }
    }
}
