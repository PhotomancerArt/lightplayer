//! What the relay writes to the account board list, and how it lands.
//!
//! The relay's hub is sans-IO and never touches the store: it decides
//! *when* a board's record changes and hands back a [`BoardRecordWrite`]
//! (a board came, its project changed, it left, its picture is due); the
//! edge applies each with [`apply_board_record_write`], once, under the
//! store lock. Every write names the accounts the board proved at the relay
//! — the only accounts whose lists it may touch.

use alloc::string::String;
use alloc::vec::Vec;
use lpc_history::PrefixedUid;
use lpc_relay::RelayBoardId;

use crate::model::board_record::{
    BoardFacts, BoardRecord, BoardSightings, MAX_ACCOUNT_BOARDS, StoredPicture,
};
use crate::ports::meta_store::MetaStore;

/// One change the relay makes to the board list. See the module doc.
#[derive(Debug, Clone, PartialEq)]
pub enum BoardRecordWrite {
    /// The board is at the relay at `at` (it registered), or was until `at`
    /// (it left, or the relay is going away): its name, firmware and, once
    /// it has reported one, its project; `seen.relay` = `at`. Joins the
    /// board to each account's list that has room.
    Seen {
        board: RelayBoardId,
        /// The accounts it proved.
        users: Vec<PrefixedUid>,
        label: String,
        /// `None` when its hello carried none (relay protocol 1): the record
        /// keeps what it knew.
        firmware: Option<String>,
        /// `None` before the board reported a project; `Some(None)` when it
        /// reported none loaded.
        project: Option<Option<String>>,
        at: f64,
    },
    /// The board reported another project at `at`. Lists that hold the
    /// board only: a project change joins no list.
    Project {
        board: RelayBoardId,
        users: Vec<PrefixedUid>,
        project: Option<String>,
        at: f64,
    },
    /// The board's last picture, written behind. Lands on the records of
    /// the accounts that may read it (`picture.accounts`), where they hold
    /// the board.
    Picture {
        board: RelayBoardId,
        picture: StoredPicture,
    },
}

/// Apply one relay write to the store. Returns the accounts the board could
/// not join because their list is full ([`MAX_ACCOUNT_BOARDS`]), for the
/// edge to log: the board is still online at the relay, the cap is the
/// list's.
///
/// The relay never knows a board's model, so a write never touches one.
pub fn apply_board_record_write<S: MetaStore + ?Sized>(
    store: &mut S,
    write: BoardRecordWrite,
) -> Vec<PrefixedUid> {
    let mut full = Vec::new();
    match write {
        BoardRecordWrite::Seen {
            board,
            users,
            label,
            firmware,
            project,
            at,
        } => {
            let facts = BoardFacts {
                label: Some(label),
                model: None,
                firmware,
                project,
                seen: BoardSightings {
                    relay: Some(at),
                    ..BoardSightings::default()
                },
            };
            for user in users {
                let record = match store.board_record(user, board) {
                    Some(mut record) => {
                        record.absorb(facts.clone());
                        record
                    }
                    None if store.board_records_for_user(user).len() < MAX_ACCOUNT_BOARDS => {
                        BoardRecord::joining(user, board, facts.clone(), at)
                    }
                    None => {
                        full.push(user);
                        continue;
                    }
                };
                store.put_board_record(record);
            }
        }
        BoardRecordWrite::Project {
            board,
            users,
            project,
            at,
        } => {
            for user in users {
                let Some(mut record) = store.board_record(user, board) else {
                    continue;
                };
                record.absorb(BoardFacts {
                    project: Some(project.clone()),
                    seen: BoardSightings {
                        relay: Some(at),
                        ..BoardSightings::default()
                    },
                    ..BoardFacts::default()
                });
                store.put_board_record(record);
            }
        }
        BoardRecordWrite::Picture { board, picture } => {
            for user in &picture.accounts {
                let Some(mut record) = store.board_record(*user, board) else {
                    continue;
                };
                record.picture = Some(picture.clone());
                store.put_board_record(record);
            }
        }
    }
    full
}

/// The picture `user` may read from its own record of `board`: only one
/// the board took while it had proved `user` (a record grants nothing).
pub fn stored_picture_for<S: MetaStore + ?Sized>(
    store: &S,
    user: PrefixedUid,
    board: RelayBoardId,
) -> Option<StoredPicture> {
    store
        .board_record(user, board)?
        .picture
        .filter(|picture| picture.accounts.contains(&user))
}
