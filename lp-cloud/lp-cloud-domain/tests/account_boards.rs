//! The account's board list as the domain answers it: `ReportBoard` adds
//! and freshens, `ForgetBoard` removes, `ListAccountBoards` reads — each for
//! the caller's own list only.

use lp_cloud_domain::{CloudService, MAX_ACCOUNT_BOARDS};
use lp_cloud_store_mem::{MemClock, MemIdMint, MemMetaStore};
use lpc_cloud_api::{
    AccountBoard, AccountBoardList, Ack, Actor, BoardSeen, CloudError, CloudRequest, CloudResponse,
    ForgetBoard, ReportBoard, SeenOn,
};

type Service = CloudService<MemMetaStore, MemClock, MemIdMint>;

const START: f64 = 1_700_000_000.0;
const LAMP: &str = "10bda3b08e30";

#[test]
fn a_report_puts_the_board_on_the_callers_list() {
    let mut svc = service();
    let alice = signed_in(&mut svc, "g-alice");

    assert_eq!(report(&mut svc, alice, lamp(SeenOn::Usb)), Ok(Ack));

    assert_eq!(
        list(&mut svc, alice),
        vec![AccountBoard {
            id: LAMP.to_string(),
            label: "Lamp".to_string(),
            model: Some("XIAO ESP32-C6".to_string()),
            firmware: Some("2026.10.20-1".to_string()),
            project: Some("Rocaille".to_string()),
            joined_at: START,
            seen: BoardSeen {
                usb: Some(START),
                ..BoardSeen::default()
            },
            online: None,
        }]
    );
}

#[test]
fn a_later_report_freshens_the_record_by_the_merge_rules() {
    let mut svc = service();
    let alice = signed_in(&mut svc, "g-alice");
    report(&mut svc, alice, lamp(SeenOn::Usb)).unwrap();
    svc.clock().advance(60.0);

    report(
        &mut svc,
        alice,
        ReportBoard {
            label: "Porch".to_string(),
            model: None,
            firmware: None,
            project: None,
            ..lamp(SeenOn::Ble)
        },
    )
    .unwrap();

    let board = list(&mut svc, alice).remove(0);
    assert_eq!(board.label, "Porch", "the board's newer name");
    assert_eq!(
        board.model.as_deref(),
        Some("XIAO ESP32-C6"),
        "a report without a model keeps the one known"
    );
    assert_eq!(
        board.firmware.as_deref(),
        Some("2026.10.20-1"),
        "an unknown firmware keeps the last one"
    );
    assert_eq!(board.project, None, "no project loaded is the newer fact");
    assert_eq!(board.joined_at, START, "joining is once");
    assert_eq!(
        board.seen,
        BoardSeen {
            usb: Some(START),
            ble: Some(START + 60.0),
            ..BoardSeen::default()
        }
    );
}

#[test]
fn a_reported_name_is_cut_to_the_relays_limit() {
    let mut svc = service();
    let alice = signed_in(&mut svc, "g-alice");
    // 31 ASCII bytes then a two-byte character straddling the 32-byte limit.
    let label = "abcdefghijklmnopqrstuvwxyz01234é".to_string();
    report(
        &mut svc,
        alice,
        ReportBoard {
            label,
            ..lamp(SeenOn::Usb)
        },
    )
    .unwrap();
    assert_eq!(
        list(&mut svc, alice)[0].label,
        "abcdefghijklmnopqrstuvwxyz01234"
    );
}

#[test]
fn a_full_list_refuses_a_new_board_and_still_freshens_one_it_holds() {
    let mut svc = service();
    let alice = signed_in(&mut svc, "g-alice");
    for n in 0..MAX_ACCOUNT_BOARDS {
        let id = format!("0200000000{n:02x}");
        report(
            &mut svc,
            alice,
            ReportBoard {
                id,
                ..lamp(SeenOn::Usb)
            },
        )
        .unwrap();
    }

    assert_eq!(
        report(&mut svc, alice, lamp(SeenOn::Usb)),
        Err(CloudError::BoardListFull { max: 64 }),
        "the 65th board"
    );
    assert_eq!(list(&mut svc, alice).len(), MAX_ACCOUNT_BOARDS);

    let held = ReportBoard {
        id: "020000000000".to_string(),
        label: "Renamed".to_string(),
        ..lamp(SeenOn::Ble)
    };
    assert_eq!(report(&mut svc, alice, held), Ok(Ack));
    assert!(
        list(&mut svc, alice)
            .iter()
            .any(|board| board.label == "Renamed")
    );
}

#[test]
fn a_guest_and_an_anonymous_caller_are_refused() {
    let mut svc = service();
    let guest = Actor::User(svc.begin_guest_user().uid);
    for actor in [guest, Actor::Anonymous] {
        assert_eq!(
            report(&mut svc, actor, lamp(SeenOn::Usb)),
            Err(CloudError::NotAuthenticated)
        );
        assert_eq!(
            svc.handle(
                actor,
                CloudRequest::ForgetBoard(ForgetBoard {
                    id: LAMP.to_string()
                })
            ),
            Err(CloudError::NotAuthenticated)
        );
    }
}

#[test]
fn a_report_naming_no_board_is_refused() {
    let mut svc = service();
    let alice = signed_in(&mut svc, "g-alice");
    let bad = ReportBoard {
        id: "lamp".to_string(),
        ..lamp(SeenOn::Usb)
    };
    assert!(matches!(
        report(&mut svc, alice, bad),
        Err(CloudError::InvalidRequest { .. })
    ));
    assert!(list(&mut svc, alice).is_empty());
}

#[test]
fn forget_removes_only_the_callers_record_and_a_second_forget_is_acknowledged() {
    let mut svc = service();
    let alice = signed_in(&mut svc, "g-alice");
    let bob = signed_in(&mut svc, "g-bob");
    report(&mut svc, alice, lamp(SeenOn::Usb)).unwrap();
    report(&mut svc, bob, lamp(SeenOn::Ble)).unwrap();

    let forget = || {
        CloudRequest::ForgetBoard(ForgetBoard {
            id: LAMP.to_string(),
        })
    };
    assert_eq!(svc.handle(alice, forget()), Ok(CloudResponse::Ack(Ack)));
    assert!(list(&mut svc, alice).is_empty());
    assert_eq!(
        list(&mut svc, bob).len(),
        1,
        "bob's record of the board stays"
    );
    assert_eq!(
        svc.handle(alice, forget()),
        Ok(CloudResponse::Ack(Ack)),
        "not on the list any more is not an error"
    );
}

#[test]
fn two_accounts_records_of_one_board_are_independent() {
    let mut svc = service();
    let alice = signed_in(&mut svc, "g-alice");
    let bob = signed_in(&mut svc, "g-bob");
    report(&mut svc, alice, lamp(SeenOn::Usb)).unwrap();
    svc.clock().advance(5.0);
    report(
        &mut svc,
        bob,
        ReportBoard {
            label: "Bob's lamp".to_string(),
            ..lamp(SeenOn::Ble)
        },
    )
    .unwrap();

    let alices = list(&mut svc, alice).remove(0);
    let bobs = list(&mut svc, bob).remove(0);
    assert_eq!(alices.label, "Lamp");
    assert_eq!(alices.seen.ble, None);
    assert_eq!(bobs.label, "Bob's lamp");
    assert_eq!(bobs.joined_at, START + 5.0);
}

fn lamp(seen_on: SeenOn) -> ReportBoard {
    ReportBoard {
        id: LAMP.to_string(),
        label: "Lamp".to_string(),
        model: Some("XIAO ESP32-C6".to_string()),
        firmware: Some("2026.10.20-1".to_string()),
        project: Some("Rocaille".to_string()),
        seen_on,
    }
}

fn report(svc: &mut Service, actor: Actor, request: ReportBoard) -> Result<Ack, CloudError> {
    svc.handle(actor, CloudRequest::ReportBoard(request))
        .map(|response| match response {
            CloudResponse::Ack(ack) => ack,
            other => panic!("{other:?}"),
        })
}

fn list(svc: &mut Service, actor: Actor) -> Vec<AccountBoard> {
    match svc.handle(actor, CloudRequest::ListAccountBoards) {
        Ok(CloudResponse::AccountBoardList(AccountBoardList { boards })) => boards,
        other => panic!("{other:?}"),
    }
}

fn service() -> Service {
    CloudService::new(MemMetaStore::new(), MemClock::new(START), MemIdMint::new())
}

fn signed_in(svc: &mut Service, google_sub: &str) -> Actor {
    let email = format!("{google_sub}@example.com");
    let user = svc.upsert_user(google_sub, &email, "Someone", "google", None, None, None);
    Actor::User(user.uid)
}
