//! The relay's writes landing on the account board list.

use lp_cloud_domain::{
    BoardRecord, BoardRecordWrite, BoardSightings, MAX_ACCOUNT_BOARDS, MetaStore, StoredPicture,
    apply_board_record_write, stored_picture_for,
};
use lp_cloud_store_mem::MemMetaStore;
use lpc_history::{PrefixedUid, UidPrefix};
use lpc_relay::RelayBoardId;

const LAMP: RelayBoardId = RelayBoardId([0x10, 0xbd, 0xa3, 0xb0, 0x8e, 0x30]);

#[test]
fn a_registration_joins_every_proving_account() {
    let mut store = MemMetaStore::new();
    let full = apply_board_record_write(&mut store, seen(&[1, 2], 10.0));
    assert!(full.is_empty());
    for n in [1, 2] {
        let record = store.board_record(user(n), LAMP).expect("joined");
        assert_eq!(record.label, "Lamp");
        assert_eq!(record.firmware.as_deref(), Some("2026.10.20-1"));
        assert_eq!(record.project, None, "not reported yet");
        assert_eq!(record.joined_at, 10.0);
        assert_eq!(record.seen.relay, Some(10.0));
    }
    assert_eq!(store.board_record(user(3), LAMP), None);
}

#[test]
fn the_relay_never_touches_the_model_and_keeps_what_it_does_not_say() {
    let mut store = MemMetaStore::new();
    store.put_board_record(BoardRecord {
        model: Some("XIAO ESP32-C6".to_string()),
        firmware: Some("2026.10.01-1".to_string()),
        project: Some("Rocaille".to_string()),
        seen: BoardSightings {
            usb: Some(5.0),
            ..BoardSightings::default()
        },
        ..record(1, 1.0)
    });
    let protocol_1 = BoardRecordWrite::Seen {
        board: LAMP,
        users: vec![user(1)],
        label: "Porch".to_string(),
        firmware: None,
        project: None,
        at: 20.0,
    };
    apply_board_record_write(&mut store, protocol_1);

    let record = store.board_record(user(1), LAMP).unwrap();
    assert_eq!(record.label, "Porch", "the board's newer name");
    assert_eq!(record.model.as_deref(), Some("XIAO ESP32-C6"));
    assert_eq!(record.firmware.as_deref(), Some("2026.10.01-1"));
    assert_eq!(record.project.as_deref(), Some("Rocaille"));
    assert_eq!(record.joined_at, 1.0);
    assert_eq!(record.seen.usb, Some(5.0));
    assert_eq!(record.seen.relay, Some(20.0));
}

#[test]
fn a_full_list_is_not_joined_and_says_so_while_another_account_is() {
    let mut store = MemMetaStore::new();
    for n in 0..MAX_ACCOUNT_BOARDS as u8 {
        store.put_board_record(BoardRecord {
            board: RelayBoardId([2, 0, 0, 0, 0, n]),
            ..record(1, 1.0)
        });
    }
    let full = apply_board_record_write(&mut store, seen(&[1, 2], 10.0));
    assert_eq!(full, [user(1)]);
    assert_eq!(store.board_record(user(1), LAMP), None);
    assert!(store.board_record(user(2), LAMP).is_some());

    // A board it already holds is freshened however full the list is.
    store.put_board_record(record(1, 1.0));
    let full = apply_board_record_write(&mut store, seen(&[1], 30.0));
    assert!(full.is_empty());
    assert_eq!(
        store.board_record(user(1), LAMP).unwrap().seen.relay,
        Some(30.0)
    );
}

#[test]
fn a_project_change_freshens_lists_that_hold_the_board_and_joins_none() {
    let mut store = MemMetaStore::new();
    apply_board_record_write(&mut store, seen(&[1], 10.0));
    apply_board_record_write(
        &mut store,
        BoardRecordWrite::Project {
            board: LAMP,
            users: vec![user(1), user(2)],
            project: Some("Rocaille".to_string()),
            at: 12.0,
        },
    );
    let record = store.board_record(user(1), LAMP).unwrap();
    assert_eq!(record.project.as_deref(), Some("Rocaille"));
    assert_eq!(record.seen.relay, Some(12.0));
    assert_eq!(store.board_record(user(2), LAMP), None);
}

#[test]
fn a_picture_lands_only_on_its_readers_records_and_only_they_read_it() {
    let mut store = MemMetaStore::new();
    apply_board_record_write(&mut store, seen(&[1, 2], 10.0));
    let picture = StoredPicture {
        at: 15.0,
        outputs: vec![3],
        colors: vec![1, 2, 3, 4, 5, 6, 7, 8, 9],
        accounts: vec![user(1), user(3)],
    };
    apply_board_record_write(
        &mut store,
        BoardRecordWrite::Picture {
            board: LAMP,
            picture: picture.clone(),
        },
    );

    assert_eq!(stored_picture_for(&store, user(1), LAMP), Some(picture));
    assert_eq!(
        store.board_record(user(2), LAMP).unwrap().picture,
        None,
        "user 2 was not proved when it was taken"
    );
    assert_eq!(stored_picture_for(&store, user(2), LAMP), None);
    assert_eq!(
        store.board_record(user(3), LAMP),
        None,
        "a picture joins nobody to a list"
    );
}

#[test]
fn a_stored_picture_whose_readers_dropped_the_holder_is_not_read() {
    let mut store = MemMetaStore::new();
    store.put_board_record(BoardRecord {
        picture: Some(StoredPicture {
            at: 15.0,
            outputs: vec![1],
            colors: vec![1, 2, 3],
            accounts: vec![user(9)],
        }),
        ..record(1, 1.0)
    });
    assert_eq!(stored_picture_for(&store, user(1), LAMP), None);
}

fn seen(users: &[u8], at: f64) -> BoardRecordWrite {
    BoardRecordWrite::Seen {
        board: LAMP,
        users: users.iter().map(|n| user(*n)).collect(),
        label: "Lamp".to_string(),
        firmware: Some("2026.10.20-1".to_string()),
        project: None,
        at,
    }
}

fn record(n: u8, joined_at: f64) -> BoardRecord {
    BoardRecord {
        user: user(n),
        board: LAMP,
        label: "Lamp".to_string(),
        model: None,
        firmware: None,
        project: None,
        joined_at,
        seen: BoardSightings::default(),
        picture: None,
    }
}

fn user(n: u8) -> PrefixedUid {
    PrefixedUid::mint(UidPrefix::User, &[n; 16])
}
