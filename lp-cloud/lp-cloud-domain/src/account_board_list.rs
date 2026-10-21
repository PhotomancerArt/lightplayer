//! The account's board list as `ListAccountBoards` answers it: records,
//! turned into the wire's rows, joined with the relay's presence.
//!
//! The domain answers from the records alone (no relay is attached to it);
//! the edge, which holds the relay, joins presence in with
//! [`with_presence`]. Both build rows the same way, here.

use alloc::string::ToString;
use alloc::vec::Vec;
use lpc_cloud_api::{AccountBoard, BoardPresence, BoardSeen, MAX_ACCOUNT_BOARDS};

use crate::model::board_record::BoardRecord;

/// One record as a row of the list, offline until presence says otherwise.
#[must_use]
pub fn account_board(record: &BoardRecord) -> AccountBoard {
    AccountBoard {
        id: record.board.to_string(),
        label: record.label.clone(),
        model: record.model.clone(),
        firmware: record.firmware.clone(),
        project: record.project.clone(),
        joined_at: record.joined_at,
        seen: BoardSeen {
            relay: record.seen.relay,
            usb: record.seen.usb,
            ble: record.seen.ble,
            lan: record.seen.lan,
        },
        online: None,
    }
}

/// Join `online` (the boards the viewer proved, as the relay sees them now)
/// onto `rows` (the viewer's records): a recorded board online now carries
/// its presence; an online board with no record yet (its first write has
/// not landed, or the list is full) is listed all the same, from its
/// presence, so an online board never disappears. Online boards come first,
/// by when they came online, then the rest by when they were last seen,
/// newest first; at most [`MAX_ACCOUNT_BOARDS`].
#[must_use]
pub fn with_presence(mut rows: Vec<AccountBoard>, online: Vec<BoardPresence>) -> Vec<AccountBoard> {
    for presence in online {
        match rows.iter_mut().find(|row| row.id == presence.id) {
            Some(row) => row.online = Some(presence),
            None => rows.push(AccountBoard {
                id: presence.id.clone(),
                label: presence.label.clone(),
                model: None,
                firmware: presence.firmware.clone(),
                project: presence.project.clone(),
                joined_at: presence.since,
                seen: BoardSeen {
                    relay: Some(presence.since),
                    ..BoardSeen::default()
                },
                online: Some(presence),
            }),
        }
    }
    rows.sort_by(|a, b| match (&a.online, &b.online) {
        (Some(a_on), Some(b_on)) => a_on
            .since
            .total_cmp(&b_on.since)
            .then_with(|| a.id.cmp(&b.id)),
        (Some(_), None) => core::cmp::Ordering::Less,
        (None, Some(_)) => core::cmp::Ordering::Greater,
        (None, None) => {
            let newest = |row: &AccountBoard| row.seen.last_seen().unwrap_or(f64::NEG_INFINITY);
            newest(b)
                .total_cmp(&newest(a))
                .then_with(|| a.id.cmp(&b.id))
        }
    });
    rows.truncate(MAX_ACCOUNT_BOARDS);
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::{String, ToString};
    use alloc::vec;

    #[test]
    fn online_boards_come_first_then_the_rest_newest_seen_first() {
        let rows = vec![
            offline("000000000001", Some(5.0)),
            offline("000000000002", Some(9.0)),
            offline("000000000003", None),
            offline("000000000004", Some(1.0)),
        ];
        let listed = with_presence(
            rows,
            vec![
                presence("000000000004", 30.0),
                presence("000000000009", 20.0),
            ],
        );
        let order: Vec<&str> = listed.iter().map(|row| row.id.as_str()).collect();
        assert_eq!(
            order,
            [
                "000000000009",
                "000000000004",
                "000000000002",
                "000000000001",
                "000000000003"
            ]
        );
    }

    #[test]
    fn a_recorded_board_online_keeps_its_record_and_gains_presence() {
        let mut row = offline("000000000001", Some(5.0));
        row.model = Some("XIAO ESP32-C6".to_string());
        let listed = with_presence(vec![row], vec![presence("000000000001", 30.0)]);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].model.as_deref(), Some("XIAO ESP32-C6"));
        assert_eq!(listed[0].joined_at, 1.0, "the record's, not the presence's");
        assert_eq!(listed[0].online.as_ref().map(|p| p.since), Some(30.0));
    }

    #[test]
    fn an_online_board_with_no_record_is_listed_from_its_presence() {
        let listed = with_presence(Vec::new(), vec![presence("000000000007", 30.0)]);
        assert_eq!(listed.len(), 1);
        let row = &listed[0];
        assert_eq!(row.label, "Board 000000000007");
        assert_eq!(row.joined_at, 30.0);
        assert_eq!(row.seen.relay, Some(30.0));
        assert_eq!(row.firmware.as_deref(), Some("2026.10.20-1"));
        assert!(row.online.is_some());
    }

    #[test]
    fn the_answer_holds_at_most_the_cap_online_boards_kept() {
        let rows = (0..MAX_ACCOUNT_BOARDS)
            .map(|n| offline(&alloc::format!("{n:012x}"), Some(n as f64)))
            .collect();
        let listed = with_presence(rows, vec![presence("ffffffffffff", 1.0)]);
        assert_eq!(listed.len(), MAX_ACCOUNT_BOARDS);
        assert_eq!(listed[0].id, "ffffffffffff");
        assert!(
            !listed.iter().any(|row| row.id == "000000000000"),
            "the least recently seen offline board is the one left out"
        );
    }

    fn offline(id: &str, seen: Option<f64>) -> AccountBoard {
        AccountBoard {
            id: id.to_string(),
            label: String::from("Lamp"),
            model: None,
            firmware: None,
            project: None,
            joined_at: 1.0,
            seen: BoardSeen {
                usb: seen,
                ..BoardSeen::default()
            },
            online: None,
        }
    }

    fn presence(id: &str, since: f64) -> BoardPresence {
        BoardPresence {
            id: id.to_string(),
            label: alloc::format!("Board {id}"),
            wire_proto: 42,
            lan: None,
            same_network: false,
            since,
            relay_proto: 2,
            firmware: Some("2026.10.20-1".to_string()),
            project: None,
        }
    }
}
