//! The relay client's rules, one test each (see `relay_client.rs`'s module
//! doc). Every test drives the sans-IO client directly with events and the
//! time, and reads back its actions.

use lpc_relay::relay_client::{
    GOING_AWAY_MAX_MS, GOING_AWAY_MIN_MS, RESOLVE_TIMEOUT_MS, VERSION_REFUSED_RETRY_MS,
};
use lpc_relay::{
    LanAddress, RefuseReason, RelayAccount, RelayAction, RelayClient, RelayClientConfig,
    RelayEvent, RelayFrame, RelayState, RouteCloseReason, verify_relay_proof,
};

const MAC: [u8; 6] = [0x10, 0xbd, 0xa3, 0xb0, 0x8e, 0x30];

#[test]
fn it_dials_only_when_joined_switched_on_and_holding_an_account() {
    // Each precondition missing, the other two present.
    let cases: [(bool, bool, bool, RelayState); 3] = [
        (true, false, true, RelayState::Off),
        (true, true, false, RelayState::NoAccount),
        (false, true, true, RelayState::WaitingForInternet),
    ];
    for (joined, cloud_relay, with_account, expected) in cases {
        let mut client = client(1);
        let mut actions = Vec::new();
        actions.extend(client.handle(0, RelayEvent::Network { joined }));
        actions.extend(client.handle(0, RelayEvent::CloudRelay(cloud_relay)));
        let accounts = if with_account {
            vec![account(1)]
        } else {
            vec![]
        };
        actions.extend(client.handle(0, RelayEvent::Accounts(accounts)));
        actions.extend(client.handle(10_000, RelayEvent::Tick));
        assert!(actions.is_empty(), "{expected}: {actions:?}");
        assert_eq!(client.state(), expected);
        assert_eq!(client.next_wake(), None);
        assert!(!client.may_dial(), "{expected}");
    }

    let mut client = client(1);
    client.handle(0, RelayEvent::Network { joined: true });
    client.handle(0, RelayEvent::CloudRelay(true));
    assert!(!client.may_dial());
    let actions = client.handle(0, RelayEvent::Accounts(vec![account(1)]));
    assert_eq!(
        actions,
        [RelayAction::Resolve {
            host: "relay.test".into()
        }]
    );
    assert_eq!(client.state(), RelayState::Connecting);
    assert!(client.may_dial());
}

#[test]
fn a_dial_resolves_connects_proves_each_account_and_registers() {
    let mut client = client(1);
    let lan = LanAddress {
        ip: [192, 168, 4, 20],
        port: 80,
    };
    client.handle(0, RelayEvent::Lan(Some(lan)));
    switch_on(&mut client, vec![account(1), account(2)]);

    assert_eq!(
        client.handle(5, RelayEvent::Resolved(Some([10, 0, 0, 1]))),
        [RelayAction::Connect {
            addr: [10, 0, 0, 1],
            port: 80
        }]
    );
    let hello = sent(&client.handle(6, RelayEvent::Connected));
    let RelayFrame::Hello(hello) = hello else {
        panic!("not a hello: {hello}");
    };
    assert_eq!(hello.board_mac, MAC);
    assert_eq!(hello.label, "Lamp");
    assert_eq!(hello.wire_proto, 39);
    assert_eq!(hello.lan, Some(lan));
    assert_eq!(hello.accounts, [[1; 16], [2; 16]]);

    let nonce = [0x77; 32];
    let proof = sent(&client.handle(7, message(&RelayFrame::Challenge { nonce })));
    let RelayFrame::Proof { proofs } = proof else {
        panic!("not a proof: {proof}");
    };
    assert_eq!(proofs.len(), 2);
    assert!(verify_relay_proof(&[1; 32], &nonce, &MAC, &proofs[0]));
    assert!(verify_relay_proof(&[2; 32], &nonce, &MAC, &proofs[1]));

    let none = client.handle(
        8,
        message(&RelayFrame::Registered {
            accounts_ok: 0b11,
            ping_s: 25,
        }),
    );
    assert!(none.is_empty());
    assert_eq!(client.state(), RelayState::Connected);
    assert_eq!(client.accounts_ok(), 0b11);
    assert_eq!(
        client.next_wake(),
        Some(8 + 60_000),
        "the silent-leg deadline"
    );
}

#[test]
fn failures_back_off_doubling_with_jitter_and_say_waiting_for_internet() {
    for (entropy, factor) in [(ZERO as fn(&mut [u8]), 0.5), (ONES, 1.5)] {
        let mut client = RelayClient::new(config(1), entropy);
        switch_on(&mut client, vec![account(1)]);
        let mut now = 0;
        let mut expected_base = 1_000.0f64;
        for _ in 0..8 {
            client.handle(now, RelayEvent::Resolved(None));
            assert_eq!(client.state(), RelayState::WaitingForInternet);
            let wake = client.next_wake().unwrap();
            let wait = (wake - now) as f64;
            let low = expected_base * 0.5;
            let high = expected_base * 1.5;
            assert!(wait >= low && wait <= high, "{wait} outside {low}..{high}");
            if factor == 0.5 {
                assert_eq!(wait, low);
            }
            now = wake;
            assert_eq!(
                client.handle(now, RelayEvent::Tick),
                [RelayAction::Resolve {
                    host: "relay.test".into()
                }]
            );
            assert_eq!(
                client.state(),
                RelayState::WaitingForInternet,
                "still unreached while it redials"
            );
            expected_base = (expected_base * 2.0).min(60_000.0);
        }
    }
}

#[test]
fn a_resolve_that_never_answers_times_out() {
    let mut client = client(1);
    switch_on(&mut client, vec![account(1)]);
    assert_eq!(client.next_wake(), Some(RESOLVE_TIMEOUT_MS));
    client.handle(RESOLVE_TIMEOUT_MS, RelayEvent::Tick);
    assert_eq!(client.state(), RelayState::WaitingForInternet);
    assert!(client.next_wake().unwrap() > RESOLVE_TIMEOUT_MS);
}

#[test]
fn going_away_waits_two_to_twelve_seconds() {
    for entropy in [ZERO as fn(&mut [u8]), ONES] {
        let mut client = RelayClient::new(config(1), entropy);
        register(&mut client, 0);
        let actions = client.handle(100, RelayEvent::Closed { going_away: true });
        assert!(actions.is_empty(), "{actions:?}");
        assert_eq!(client.state(), RelayState::Connecting);
        let wait = client.next_wake().unwrap() - 100;
        assert!(
            (GOING_AWAY_MIN_MS..=GOING_AWAY_MAX_MS).contains(&wait),
            "{wait}"
        );
    }
}

#[test]
fn a_drop_after_registering_backs_off_and_says_connecting() {
    let mut client = client(1);
    register(&mut client, 0);
    client.handle(100, RelayEvent::Closed { going_away: false });
    assert_eq!(client.state(), RelayState::Connecting);
    assert_eq!(client.next_wake(), Some(100 + 500));
}

#[test]
fn an_unknown_account_waits_until_the_accounts_change() {
    let mut client = client(1);
    registering(&mut client, 0);
    let actions = client.handle(
        10,
        message(&RelayFrame::Refused {
            reason: RefuseReason::UnknownAccount,
            retry_after_s: 0,
        }),
    );
    assert_eq!(actions, [RelayAction::Close]);
    assert_eq!(
        client.state(),
        RelayState::Refused {
            reason: RefuseReason::UnknownAccount
        }
    );
    assert_eq!(client.next_wake(), None);
    assert!(client.handle(10_000_000, RelayEvent::Tick).is_empty());
    assert!(
        client
            .handle(10_000_001, RelayEvent::Accounts(vec![account(1)]))
            .is_empty(),
        "the same entries again change nothing"
    );

    let actions = client.handle(10_000_002, RelayEvent::Accounts(vec![account(9)]));
    assert_eq!(
        actions,
        [RelayAction::Resolve {
            host: "relay.test".into()
        }]
    );
    assert_eq!(client.state(), RelayState::Connecting);
}

#[test]
fn a_version_refusal_waits_an_hour() {
    for reason in [RefuseReason::VersionTooOld, RefuseReason::VersionTooNew] {
        let mut client = client(1);
        registering(&mut client, 0);
        client.handle(
            10,
            message(&RelayFrame::Refused {
                reason,
                retry_after_s: 0,
            }),
        );
        assert_eq!(client.state(), RelayState::Refused { reason });
        assert_eq!(client.next_wake(), Some(10 + VERSION_REFUSED_RETRY_MS));
    }
}

#[test]
fn a_busy_refusal_waits_at_least_as_long_as_the_hub_asks() {
    for reason in [
        RefuseReason::Busy,
        RefuseReason::TooManyBoards,
        RefuseReason::Malformed,
    ] {
        let mut client = client(1);
        registering(&mut client, 0);
        client.handle(
            10,
            message(&RelayFrame::Refused {
                reason,
                retry_after_s: 120,
            }),
        );
        assert_eq!(client.state(), RelayState::Refused { reason });
        assert_eq!(client.next_wake(), Some(10 + 120_000));
        assert_eq!(
            client.handle(10 + 120_000, RelayEvent::Tick),
            [RelayAction::Resolve {
                host: "relay.test".into()
            }]
        );
    }
}

#[test]
fn losing_wifi_closes_the_leg_and_its_routes_and_stops_dialling() {
    let mut client = client(1);
    register(&mut client, 0);
    client.handle(10, message(&RelayFrame::Open { route: 3 }));
    let actions = client.handle(20, RelayEvent::Network { joined: false });
    assert_eq!(actions, [RelayAction::Close, RelayAction::RouteClosed(3)]);
    assert_eq!(client.state(), RelayState::WaitingForInternet);
    assert_eq!(client.next_wake(), None);
    assert!(client.handle(1_000_000, RelayEvent::Tick).is_empty());
    assert!(
        client
            .handle(1_000_001, RelayEvent::Closed { going_away: false })
            .is_empty(),
        "a late close of the dropped leg is ignored"
    );
}

#[test]
fn cloud_relay_off_mid_session_closes_and_never_dials() {
    let mut client = client(1);
    register(&mut client, 0);
    assert!(client.may_dial());
    let actions = client.handle(20, RelayEvent::CloudRelay(false));
    assert_eq!(actions, [RelayAction::Close]);
    assert_eq!(client.state(), RelayState::Off);
    assert!(
        !client.may_dial(),
        "off: the edge may give the buffers back"
    );
    assert_eq!(client.next_wake(), None);
    for now in [100, 10_000, 1_000_000] {
        assert!(client.handle(now, RelayEvent::Tick).is_empty());
    }
    assert!(
        client
            .handle(1_000_001, RelayEvent::Network { joined: true })
            .is_empty()
    );
}

#[test]
fn a_board_of_one_session_answers_a_second_open_busy() {
    let mut client = client(1);
    register(&mut client, 0);
    assert_eq!(
        client.handle(10, message(&RelayFrame::Open { route: 1 })),
        [RelayAction::RouteOpened(1)]
    );
    let refusal = sent(&client.handle(11, message(&RelayFrame::Open { route: 2 })));
    assert_eq!(
        refusal,
        RelayFrame::Close {
            route: 2,
            reason: RouteCloseReason::Busy
        }
    );
    assert_eq!(client.routes().len(), 1);
}

#[test]
fn frames_pass_both_ways_byte_identical() {
    let mut client = client(2);
    register(&mut client, 0);
    client.handle(10, message(&RelayFrame::Open { route: 1 }));
    let lp_link_frame = vec![0xa5, 0x00, 0xff, 0x10, 0x20];
    assert_eq!(
        client.handle(
            11,
            message(&RelayFrame::Frame {
                route: 1,
                bytes: lp_link_frame.clone()
            })
        ),
        [RelayAction::RouteFrame {
            route: 1,
            bytes: lp_link_frame.clone()
        }]
    );
    assert_eq!(
        sent(&client.handle(
            12,
            RelayEvent::RouteSend {
                route: 1,
                bytes: &lp_link_frame
            }
        )),
        RelayFrame::Frame {
            route: 1,
            bytes: lp_link_frame
        }
    );
    assert!(
        client
            .handle(
                13,
                RelayEvent::RouteSend {
                    route: 9,
                    bytes: &[1]
                }
            )
            .is_empty(),
        "nothing goes out on a route the board does not hold"
    );
    assert_eq!(
        client.handle(
            14,
            message(&RelayFrame::Close {
                route: 1,
                reason: RouteCloseReason::Normal
            })
        ),
        [RelayAction::RouteClosed(1)]
    );
    client.handle(15, message(&RelayFrame::Open { route: 4 }));
    assert_eq!(
        sent(&client.handle(
            16,
            RelayEvent::RouteClose {
                route: 4,
                reason: RouteCloseReason::Normal
            }
        )),
        RelayFrame::Close {
            route: 4,
            reason: RouteCloseReason::Normal
        }
    );
    // A board that turns a newcomer away (its one session is held) says so.
    client.handle(17, message(&RelayFrame::Open { route: 5 }));
    assert_eq!(
        sent(&client.handle(
            18,
            RelayEvent::RouteClose {
                route: 5,
                reason: RouteCloseReason::Busy
            }
        )),
        RelayFrame::Close {
            route: 5,
            reason: RouteCloseReason::Busy
        }
    );
}

#[test]
fn a_silent_leg_is_closed_and_a_ping_keeps_it_alive() {
    let mut client = client(1);
    register(&mut client, 0);
    client.handle(50_000, RelayEvent::Heard);
    assert!(client.handle(60_000, RelayEvent::Tick).is_empty());
    assert_eq!(client.next_wake(), Some(110_000));
    assert_eq!(
        client.handle(110_000, RelayEvent::Tick),
        [RelayAction::Close]
    );
    assert_eq!(client.state(), RelayState::Connecting);
}

#[test]
fn new_accounts_while_registered_register_again() {
    let mut client = client(1);
    register(&mut client, 0);
    let actions = client.handle(10, RelayEvent::Accounts(vec![account(1), account(2)]));
    assert_eq!(
        actions,
        [
            RelayAction::Close,
            RelayAction::Resolve {
                host: "relay.test".into()
            }
        ]
    );
}

#[test]
fn a_lan_change_while_registered_is_sent() {
    let mut client = client(1);
    register(&mut client, 0);
    let lan = Some(LanAddress {
        ip: [10, 0, 0, 9],
        port: 80,
    });
    assert_eq!(
        sent(&client.handle(10, RelayEvent::Lan(lan))),
        RelayFrame::LanChanged { lan }
    );
    assert!(client.handle(11, RelayEvent::Lan(lan)).is_empty());
}

#[test]
fn garbage_on_the_leg_closes_it_and_backs_off() {
    let mut client = client(1);
    register(&mut client, 0);
    assert_eq!(
        client.handle(10, RelayEvent::Message(&[0xee, 1, 2])),
        [RelayAction::Close]
    );
    assert_eq!(client.state(), RelayState::Connecting);
    assert!(client.next_wake().is_some());
}

#[test]
fn a_late_socket_nobody_wants_is_closed() {
    let mut client = client(1);
    switch_on(&mut client, vec![account(1)]);
    client.handle(1, RelayEvent::Resolved(Some([1, 1, 1, 1])));
    client.handle(2, RelayEvent::CloudRelay(false));
    assert_eq!(
        client.handle(3, RelayEvent::Connected),
        [RelayAction::Close]
    );
}

// ---- helpers ---------------------------------------------------------

const ZERO: fn(&mut [u8]) = |bytes| bytes.fill(0);
const ONES: fn(&mut [u8]) = |bytes| bytes.fill(0xff);

fn config(max_routes: usize) -> RelayClientConfig {
    RelayClientConfig {
        host: "relay.test".into(),
        port: 80,
        board_mac: MAC,
        label: "Lamp".into(),
        wire_proto: 39,
        max_routes,
    }
}

fn client(max_routes: usize) -> RelayClient {
    RelayClient::new(config(max_routes), ZERO)
}

fn account(n: u8) -> RelayAccount {
    RelayAccount {
        salt: [n; 16],
        k: [n; 32],
    }
}

fn message(frame: &RelayFrame) -> RelayEvent<'static> {
    RelayEvent::Message(Box::leak(frame.encode().into_boxed_slice()))
}

/// The one frame `actions` sends.
fn sent(actions: &[RelayAction]) -> RelayFrame {
    match actions {
        [RelayAction::Send(bytes)] => RelayFrame::decode(bytes).expect("a relay frame"),
        other => panic!("expected one Send, got {other:?}"),
    }
}

fn switch_on(client: &mut RelayClient, accounts: Vec<RelayAccount>) {
    client.handle(0, RelayEvent::Network { joined: true });
    client.handle(0, RelayEvent::CloudRelay(true));
    client.handle(0, RelayEvent::Accounts(accounts));
}

/// Up to the hello: the leg is open and the hub has not answered.
fn registering(client: &mut RelayClient, now: u64) {
    switch_on(client, vec![account(1)]);
    client.handle(now, RelayEvent::Resolved(Some([10, 0, 0, 1])));
    client.handle(now, RelayEvent::Connected);
}

fn register(client: &mut RelayClient, now: u64) {
    registering(client, now);
    client.handle(now, message(&RelayFrame::Challenge { nonce: [1; 32] }));
    client.handle(
        now,
        message(&RelayFrame::Registered {
            accounts_ok: 1,
            ping_s: 25,
        }),
    );
    assert_eq!(client.state(), RelayState::Connected);
}
