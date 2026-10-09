//! The relay client's rules, one test each (see `relay_client.rs`'s module
//! doc). Every test drives the sans-IO client directly with events and the
//! time, and reads back its actions.

use lpc_relay::relay_client::{
    GOING_AWAY_MAX_MS, GOING_AWAY_MIN_MS, RESOLVE_TIMEOUT_MS, VERSION_REFUSED_RETRY_MS,
    VERSION_TOO_NEW_RETRY_MS,
};
use lpc_relay::{
    LanAddress, PictureRate, RELAY_PROTO_2, RefuseReason, RelayAccount, RelayAction, RelayClient,
    RelayClientConfig, RelayEvent, RelayFrame, RelayPicture, RelayProject, RelayProjectFacts,
    RelayState, RouteCloseReason, project_content_tag, project_tag_key, project_uid_tag,
    verify_relay_proof,
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

/// Too old: the board needs an update, so it asks again in an hour.
#[test]
fn a_too_old_refusal_waits_an_hour() {
    let reason = RefuseReason::VersionTooOld;
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

/// Too new: the hub is behind the board (a deploy in progress, a
/// rollback), which passes, so it asks again in five minutes — at least
/// as long as the hub says.
#[test]
fn a_too_new_refusal_waits_five_minutes() {
    let reason = RefuseReason::VersionTooNew;
    for (retry_after_s, wait) in [(0, VERSION_TOO_NEW_RETRY_MS), (600, 600_000)] {
        let mut client = client(1);
        registering(&mut client, 0);
        client.handle(
            10,
            message(&RelayFrame::Refused {
                reason,
                retry_after_s,
            }),
        );
        assert_eq!(client.state(), RelayState::Refused { reason });
        assert_eq!(client.next_wake(), Some(10 + wait));
    }
    assert_eq!(VERSION_TOO_NEW_RETRY_MS, 5 * 60 * 1000);
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

#[test]
fn the_hello_is_protocol_2_and_carries_the_firmware() {
    let mut client = client(1);
    switch_on(&mut client, vec![account(1)]);
    client.handle(1, RelayEvent::Resolved(Some([10, 0, 0, 1])));
    let hello = sent(&client.handle(2, RelayEvent::Connected));
    let RelayFrame::Hello(hello) = hello else {
        panic!("not a hello: {hello}");
    };
    assert_eq!(hello.relay_proto, RELAY_PROTO_2);
    assert_eq!(hello.firmware.as_deref(), Some("test-1"));
}

#[test]
fn no_picture_is_taken_before_the_hub_asks() {
    let mut client = client(1);
    register(&mut client, 0);
    for now in [1, 1_000, 30_000, 59_000] {
        client.handle(now, RelayEvent::Heard);
        assert!(client.handle(now, RelayEvent::Tick).is_empty(), "{now}");
    }
    assert_eq!(client.next_picture_due(), None);
    assert_eq!(
        client.next_wake(),
        Some(59_000 + 60_000),
        "only the silence"
    );
    assert_eq!(
        client.handle(59_001, RelayEvent::PictureReady),
        [RelayAction::DropPicture],
        "a picture nobody asked for"
    );
}

#[test]
fn a_picture_rate_takes_one_at_once_then_one_a_minute_idle() {
    let mut client = client(1);
    register(&mut client, 0);
    assert_eq!(
        client.handle(10, message(&rate(60, 500, 0))),
        [RelayAction::TakePicture]
    );
    assert_eq!(
        client.handle(20, RelayEvent::PictureReady),
        [RelayAction::SendPicture]
    );
    assert!(!client.pictures_watched(20));
    let times = picture_times(&mut client, 20, 200_000);
    assert_eq!(times, [60_010, 120_010, 180_010]);
}

#[test]
fn watched_pictures_come_every_half_second_until_the_watch_ends() {
    let mut client = client(1);
    register(&mut client, 0);
    assert_eq!(
        client.handle(0, message(&rate(60, 500, 15))),
        [RelayAction::TakePicture]
    );
    client.handle(0, RelayEvent::PictureReady);
    assert!(client.pictures_watched(14_999));
    assert!(!client.pictures_watched(15_000));
    let times = picture_times(&mut client, 0, 80_000);
    let watched: Vec<u64> = (1..30).map(|n| n * 500).collect();
    assert_eq!(times[..29], watched[..], "every 500 ms until 15 s");
    assert_eq!(
        times[29..],
        [14_500 + 60_000],
        "then idle: a minute after the last"
    );
}

#[test]
fn the_rate_is_clamped() {
    // watched_ms 0 → 250; idle_s 5 → 10; watched_for_s 1000 → 300.
    let mut client = client(1);
    register(&mut client, 0);
    assert_eq!(
        client.handle(0, message(&rate(5, 0, 1000))),
        [RelayAction::TakePicture]
    );
    client.handle(0, RelayEvent::PictureReady);
    assert_eq!(client.next_picture_due(), Some(250));
    assert!(client.pictures_watched(299_999));
    assert!(!client.pictures_watched(300_000));
    let times = picture_times(&mut client, 0, 320_000);
    assert_eq!(times.len(), 1199 + 2, "{:?}", &times[1190..]);
    assert_eq!(times[0], 250);
    assert_eq!(times[1198], 299_750, "the last watched one");
    assert_eq!(times[1199..], [309_750, 319_750], "then every 10 s");

    // idle_s 0 stays "none": one at once, then nothing.
    let mut client = client_registered();
    assert_eq!(
        client.handle(0, message(&rate(0, 500, 0))),
        [RelayAction::TakePicture]
    );
    client.handle(0, RelayEvent::PictureReady);
    assert_eq!(client.next_picture_due(), None);
    assert!(picture_times(&mut client, 0, 200_000).is_empty());
}

#[test]
fn one_picture_in_flight_and_an_edge_that_never_answers_is_asked_again_at_the_next_due_time() {
    let mut client = client(1);
    register(&mut client, 0);
    assert_eq!(
        client.handle(0, message(&rate(60, 500, 15))),
        [RelayAction::TakePicture]
    );
    // Not answered. A renewal while it is in flight asks for no second one…
    assert!(client.handle(100, message(&rate(60, 500, 15))).is_empty());
    assert_eq!(client.next_wake(), Some(600));
    for now in [200, 400, 599] {
        assert!(client.handle(now, RelayEvent::Tick).is_empty(), "{now}");
    }
    // …and the next due time asks again, never sooner.
    assert_eq!(
        client.handle(600, RelayEvent::Tick),
        [RelayAction::TakePicture]
    );
    assert_eq!(client.next_wake(), Some(1_100));
    assert_eq!(
        client.handle(700, RelayEvent::PictureReady),
        [RelayAction::SendPicture]
    );
    assert_eq!(
        client.handle(701, RelayEvent::PictureReady),
        [RelayAction::DropPicture],
        "one answer per ask"
    );
}

#[test]
fn a_picture_ready_after_the_leg_dropped_is_dropped() {
    let mut client = client(1);
    register(&mut client, 0);
    assert_eq!(
        client.handle(0, message(&rate(60, 500, 15))),
        [RelayAction::TakePicture]
    );
    client.handle(10, RelayEvent::Closed { going_away: false });
    assert_eq!(
        client.handle(20, RelayEvent::PictureReady),
        [RelayAction::DropPicture]
    );
    assert_eq!(client.next_picture_due(), None);
}

#[test]
fn re_registering_clears_the_schedule_until_the_next_rate() {
    let mut client = client(1);
    register(&mut client, 0);
    client.handle(0, message(&rate(60, 500, 15)));
    client.handle(0, RelayEvent::PictureReady);
    assert_eq!(client.next_picture_due(), Some(500));

    // A deploy: the hub goes away, the board comes back.
    client.handle(100, RelayEvent::Closed { going_away: true });
    assert_eq!(client.next_picture_due(), None, "the leg went");
    let redial = client.next_wake().unwrap();
    client.handle(redial, RelayEvent::Tick);
    client.handle(redial, RelayEvent::Resolved(Some([10, 0, 0, 1])));
    client.handle(redial, RelayEvent::Connected);
    client.handle(redial, message(&RelayFrame::Challenge { nonce: [1; 32] }));
    client.handle(
        redial,
        message(&RelayFrame::Registered {
            accounts_ok: 1,
            ping_s: 25,
        }),
    );
    assert_eq!(client.state(), RelayState::Connected);
    assert_eq!(client.next_picture_due(), None);
    assert!(picture_times(&mut client, redial, redial + 200_000).is_empty());

    // The new hub asks: one at once.
    let now = redial + 200_001;
    client.handle(now, RelayEvent::Heard);
    assert_eq!(
        client.handle(now, message(&rate(60, 500, 0))),
        [RelayAction::TakePicture]
    );
}

#[test]
fn the_project_goes_after_registered_and_on_every_change() {
    let mut client = client(1);
    let rocaille = facts("Rocaille", None, None);
    assert!(
        client
            .handle(0, RelayEvent::Project(Some(rocaille.clone())))
            .is_empty(),
        "not registered: kept for later"
    );
    registering(&mut client, 0);
    client.handle(0, message(&RelayFrame::Challenge { nonce: [1; 32] }));
    let after_registered = client.handle(
        0,
        message(&RelayFrame::Registered {
            accounts_ok: 1,
            ping_s: 25,
        }),
    );
    assert_eq!(
        sent(&after_registered),
        RelayFrame::Project(Some(RelayProject {
            name: "Rocaille".into(),
            uid_tag: None,
            content_tag: None,
        }))
    );

    // A change while registered, and no project at all.
    let long = "A project whose name is longer than thirty-two bytes";
    assert_eq!(
        sent(&client.handle(10, RelayEvent::Project(Some(facts(long, None, None))))),
        RelayFrame::Project(Some(RelayProject {
            name: long[..32].into(),
            uid_tag: None,
            content_tag: None,
        }))
    );
    assert_eq!(
        sent(&client.handle(20, RelayEvent::Project(None))),
        RelayFrame::Project(None)
    );

    // Kept across a reconnect, and sent again after the next Registered.
    client.handle(30, RelayEvent::Closed { going_away: true });
    let redial = client.next_wake().unwrap();
    client.handle(redial, RelayEvent::Tick);
    client.handle(redial, RelayEvent::Resolved(Some([10, 0, 0, 1])));
    client.handle(redial, RelayEvent::Connected);
    client.handle(redial, message(&RelayFrame::Challenge { nonce: [1; 32] }));
    assert_eq!(
        sent(&client.handle(
            redial,
            message(&RelayFrame::Registered {
                accounts_ok: 1,
                ping_s: 25,
            }),
        )),
        RelayFrame::Project(None)
    );
}

#[test]
fn the_project_tags_use_the_first_verified_account() {
    let uid = "prj7m3qk2x9z4w8v6t5r1n0p2a4c";
    let hash = [0x11; 32];
    let mut client = client(1);
    client.handle(
        0,
        RelayEvent::Project(Some(facts("Rocaille", Some(uid), Some(hash)))),
    );
    switch_on(&mut client, vec![account(1), account(2)]);
    client.handle(0, RelayEvent::Resolved(Some([10, 0, 0, 1])));
    client.handle(0, RelayEvent::Connected);
    client.handle(0, message(&RelayFrame::Challenge { nonce: [1; 32] }));
    let report = sent(&client.handle(
        0,
        message(&RelayFrame::Registered {
            accounts_ok: 0b10,
            ping_s: 25,
        }),
    ));
    // Account 1 did not verify; account 2 (k = [2; 32]) is the first that did.
    let key = project_tag_key(&[2; 32]);
    assert_eq!(
        report,
        RelayFrame::Project(Some(RelayProject {
            name: "Rocaille".into(),
            uid_tag: Some(project_uid_tag(&key, uid)),
            content_tag: Some(project_content_tag(&key, &hash)),
        }))
    );
    let wrong = project_tag_key(&[1; 32]);
    assert_ne!(project_uid_tag(&key, uid), project_uid_tag(&wrong, uid));
}

#[test]
fn the_uid_and_the_hash_never_leave_the_client() {
    let uid = "prj7m3qk2x9z4w8v6t5r1n0p2a4c";
    let hash = [0x5e; 32];
    let mut client = client(1);
    let mut sent_bytes: Vec<Vec<u8>> = Vec::new();
    let mut keep = |actions: Vec<RelayAction>| {
        for action in actions {
            if let RelayAction::Send(bytes) = action {
                sent_bytes.push(bytes);
            }
        }
    };
    keep(client.handle(
        0,
        RelayEvent::Project(Some(facts("Rocaille", Some(uid), Some(hash)))),
    ));
    keep(client.handle(0, RelayEvent::Network { joined: true }));
    keep(client.handle(0, RelayEvent::CloudRelay(true)));
    keep(client.handle(0, RelayEvent::Accounts(vec![account(1), account(2)])));
    for round in 0..2u64 {
        let t = round * 1_000_000;
        keep(client.handle(t, RelayEvent::Tick));
        keep(client.handle(t, RelayEvent::Resolved(Some([10, 0, 0, 1]))));
        keep(client.handle(t, RelayEvent::Connected));
        keep(client.handle(t, message(&RelayFrame::Challenge { nonce: [9; 32] })));
        keep(client.handle(
            t,
            message(&RelayFrame::Registered {
                accounts_ok: 0b11,
                ping_s: 25,
            }),
        ));
        keep(client.handle(t + 1, message(&rate(60, 500, 15))));
        keep(client.handle(t + 2, RelayEvent::PictureReady));
        keep(client.handle(
            t + 3,
            RelayEvent::Project(Some(facts("Rocaille 2", Some(uid), Some(hash)))),
        ));
        keep(client.handle(t + 4, message(&RelayFrame::Open { route: 1 })));
        keep(client.handle(
            t + 5,
            RelayEvent::RouteSend {
                route: 1,
                bytes: &[1, 2, 3],
            },
        ));
        keep(client.handle(t + 501, RelayEvent::Tick));
        keep(client.handle(t + 502, RelayEvent::PictureReady));
        // The hub drops the leg; the board dials again (the next round).
        keep(client.handle(t + 600, RelayEvent::Closed { going_away: true }));
    }
    let reports = sent_bytes
        .iter()
        .filter(|bytes| matches!(RelayFrame::decode(bytes), Ok(RelayFrame::Project(Some(_)))))
        .count();
    assert_eq!(
        reports, 4,
        "two reports a round: after Registered, on change"
    );
    for bytes in &sent_bytes {
        assert!(
            !contains(bytes, uid.as_bytes()),
            "the uid crossed: {bytes:x?}"
        );
        assert!(!contains(bytes, &hash), "the hash crossed: {bytes:x?}");
        assert!(!contains(bytes, &hash[..8]), "part of the hash crossed");
    }
}

#[test]
fn the_hub_cannot_send_project_or_picture() {
    for frame in [
        RelayFrame::Project(None),
        RelayFrame::Picture(RelayPicture {
            outputs: vec![1],
            colors: vec![1, 2, 3],
        }),
    ] {
        let mut client = client(1);
        register(&mut client, 0);
        assert_eq!(
            client.handle(10, message(&frame)),
            [RelayAction::Close],
            "{frame}"
        );
        assert_eq!(client.state(), RelayState::Connecting, "{frame}");
    }
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
        firmware: "test-1".into(),
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

fn client_registered() -> RelayClient {
    let mut client = client(1);
    register(&mut client, 0);
    client
}

fn rate(idle_s: u16, watched_ms: u16, watched_for_s: u16) -> RelayFrame {
    RelayFrame::PictureRate(PictureRate {
        idle_s,
        watched_ms,
        watched_for_s,
    })
}

fn facts(name: &str, uid: Option<&str>, content_hash: Option<[u8; 32]>) -> RelayProjectFacts {
    RelayProjectFacts {
        name: name.into(),
        uid: uid.map(Into::into),
        content_hash,
    }
}

/// Run the client from `from` to `until`, waking when it asks, the leg kept
/// alive (the hub pings), each picture answered at once: when it asked.
fn picture_times(client: &mut RelayClient, from: u64, until: u64) -> Vec<u64> {
    let mut times = Vec::new();
    while let Some(wake) = client.next_wake() {
        assert!(wake >= from, "a wake in the past: {wake}");
        if wake > until {
            break;
        }
        client.handle(wake, RelayEvent::Heard);
        let actions = client.handle(wake, RelayEvent::Tick);
        assert!(
            actions
                .iter()
                .all(|action| matches!(action, RelayAction::TakePicture)),
            "{actions:?}"
        );
        if !actions.is_empty() {
            times.push(wake);
            assert_eq!(
                client.handle(wake, RelayEvent::PictureReady),
                [RelayAction::SendPicture]
            );
        }
    }
    times
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}
