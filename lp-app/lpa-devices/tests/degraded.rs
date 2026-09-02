//! A running board that is not running WELL says so.
//!
//! The bench case this exists for (2026-09-01): a C6 whose only compute
//! node had been quarantined rendered black for two days while the card
//! read "Running", because the heartbeat mirror dropped both the recovery
//! state and the project's fault. Everything here runs through the crate's
//! public surface — a claim the app cannot make is not a claim.

use lpa_devices::view::roster_view;
use lpa_devices::wire::{
    LoadedProjectFacts, ProjectFaultFacts, RecoveryFacts, RecoveryLevelFacts, RecoveryPathFacts,
    ServerFrame,
};
use lpa_devices::{
    DeviceStatus, HelloFacts, Input, LinkEvent, LinkId, LinkInfo, Millis, PeerIdentity, Roster,
    RosterConfig,
};

fn identified_board() -> Roster {
    let config = RosterConfig::default();
    let hello = HelloFacts {
        proto: config.expected_proto,
        board_id: Some("seeed-xiao-esp32c6".to_string()),
        // A uid is what promotes a pending link to a device — an anonymous
        // board stays in the roster's identifying lane.
        identity: PeerIdentity {
            uid: Some(lpa_devices::DeviceUid("dev_2f8a".to_string())),
            ..Default::default()
        },
        ..Default::default()
    };
    let mut roster = Roster::new(config);
    roster.handle(
        Millis(0),
        Input::Event(lpa_devices::Event::LinkAttached {
            link: LinkId(1),
            info: LinkInfo::default(),
        }),
    );
    roster.handle(
        Millis(10),
        Input::link(
            LinkId(1),
            LinkEvent::Opened {
                info: LinkInfo::default(),
            },
        ),
    );
    roster.handle(
        Millis(100),
        Input::link(LinkId(1), LinkEvent::Frame(ServerFrame::hello(1, hello))),
    );
    roster
}

fn heartbeat(
    roster: &mut Roster,
    at: Millis,
    loaded: Option<Vec<LoadedProjectFacts>>,
    recovery: Option<RecoveryFacts>,
) {
    roster.handle(
        at,
        Input::link(
            LinkId(1),
            LinkEvent::Frame(ServerFrame::heartbeat_report(
                Some(PeerIdentity::default()),
                loaded,
                recovery,
            )),
        ),
    );
}

fn quarantined() -> ProjectFaultFacts {
    ProjectFaultFacts::node(
        "/meteor.show/s",
        "recovery: node 'nodes/meteor' (disabled after 3 crashes)",
    )
}

#[test]
fn a_faulted_project_reads_degraded_and_still_says_what_it_is_running() {
    let mut roster = identified_board();
    heartbeat(
        &mut roster,
        Millis(200),
        Some(vec![LoadedProjectFacts::faulted(
            "/projects/meteor",
            quarantined(),
        )]),
        None,
    );

    let card = roster_view(&roster, Millis(300)).devices[0].clone();
    assert_eq!(card.status, DeviceStatus::Degraded, "{card:?}");
    assert_eq!(card.state_label, "Degraded");
    // The running face SURVIVES: a degraded board is still running, and a
    // card that dropped the project name would answer "what is on it?"
    // with a complaint.
    assert_eq!(
        card.loaded_project,
        lpa_devices::view::LoadedProject::Running {
            label: "meteor".to_string()
        }
    );
    let line = card.degraded.expect("the card names the degradation");
    assert!(
        line.starts_with("Degraded: node /meteor.show/s faulted"),
        "{line}"
    );
    assert!(line.contains("disabled after 3 crashes"), "{line}");
}

#[test]
fn a_clean_heartbeat_clears_the_degraded_face() {
    let mut roster = identified_board();
    heartbeat(
        &mut roster,
        Millis(200),
        Some(vec![LoadedProjectFacts::faulted(
            "/projects/meteor",
            quarantined(),
        )]),
        Some(RecoveryFacts {
            level: RecoveryLevelFacts::Red,
            safe_mode: false,
            paths: vec![RecoveryPathFacts {
                label: "node:nodes/meteor".to_string(),
                gated: true,
            }],
            last_crash: None,
        }),
    );
    assert_eq!(
        roster_view(&roster, Millis(250)).devices[0].status,
        DeviceStatus::Degraded
    );

    // The board recovers and says so: green, no fault. The face must go
    // back — a status that could only ever get worse is a latch, and the
    // whole model refuses latched verdicts.
    heartbeat(
        &mut roster,
        Millis(1_200),
        Some(vec![LoadedProjectFacts::new("/projects/meteor")]),
        Some(RecoveryFacts::default()),
    );

    let card = roster_view(&roster, Millis(1_300)).devices[0].clone();
    assert_eq!(card.status, DeviceStatus::Ready, "{card:?}");
    assert_eq!(card.degraded, None);
}

#[test]
fn a_heartbeat_that_says_nothing_about_recovery_does_not_clear_it() {
    // The fold rule the phase turns on: absence is "did not say", never
    // "green". Host servers and the browser sim install no recovery region
    // at all, and old firmware sent none — reading either as healthy is the
    // over-claim this whole feature exists to stop.
    let mut roster = identified_board();
    heartbeat(
        &mut roster,
        Millis(200),
        Some(vec![LoadedProjectFacts::new("/projects/porch")]),
        Some(RecoveryFacts {
            level: RecoveryLevelFacts::Yellow,
            safe_mode: false,
            paths: vec![RecoveryPathFacts {
                label: "node:nodes/fire".to_string(),
                gated: false,
            }],
            last_crash: Some("oom at node:nodes/fire".to_string()),
        }),
    );

    // A later heartbeat carrying no recovery block at all.
    heartbeat(
        &mut roster,
        Millis(1_200),
        Some(vec![LoadedProjectFacts::new("/projects/porch")]),
        None,
    );

    let card = roster_view(&roster, Millis(1_300)).devices[0].clone();
    assert_eq!(card.status, DeviceStatus::Degraded, "{card:?}");
    assert_eq!(
        card.degraded.as_deref(),
        Some("Recovery yellow: 1 watched path (last crash: oom at node:nodes/fire)"),
    );
}

#[test]
fn safe_mode_alone_is_worth_saying() {
    let mut roster = identified_board();
    heartbeat(
        &mut roster,
        Millis(200),
        Some(vec![LoadedProjectFacts::new("/projects/porch")]),
        Some(RecoveryFacts {
            level: RecoveryLevelFacts::Green,
            safe_mode: true,
            paths: Vec::new(),
            last_crash: None,
        }),
    );

    let card = roster_view(&roster, Millis(300)).devices[0].clone();
    assert_eq!(card.status, DeviceStatus::Degraded);
    assert_eq!(
        card.degraded.as_deref(),
        Some("Safe mode: project auto-load skipped after repeated incomplete boots"),
    );
}

#[test]
fn the_degraded_line_is_the_same_line_every_heartbeat() {
    // It sits on a card that redraws every second; a line that reshuffled
    // its clauses would read as new news forever.
    let mut roster = identified_board();
    let mut seen = Vec::new();
    for (index, at) in [200u64, 1_200, 2_200].into_iter().enumerate() {
        heartbeat(
            &mut roster,
            Millis(at),
            Some(vec![LoadedProjectFacts::faulted(
                "/projects/meteor",
                quarantined(),
            )]),
            None,
        );
        seen.push(
            roster_view(&roster, Millis(at + 10)).devices[0]
                .degraded
                .clone(),
        );
        assert!(seen[index].is_some());
    }
    assert_eq!(seen[0], seen[1]);
    assert_eq!(seen[1], seen[2]);
}
