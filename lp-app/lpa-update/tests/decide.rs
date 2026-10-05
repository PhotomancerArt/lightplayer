//! The decision: one test per host-side row of the roadmap's E1–E14 (E4,
//! E11 and E14 have no host row; E13's miss is the engine source's, tested
//! there and in the simulation).

use lpa_update::{BoardView, Decision, HostBuild, HostFacts, HostIdentity, NeedsUsbWhy, decide};
use lpc_access::Tier;
use lpc_update::{BoardManifest, BoardState, PieceKind, TransferView, sha256_to_hex};

/// The host's build Y.
fn y() -> HostBuild {
    HostBuild::from_parts(
        HostIdentity {
            target: "esp32c6-4mb".into(),
            chip: "esp32c6".into(),
            version: "2026.10.06-1".into(),
            build_id: "2026.10.06-1+bbbbbbbbbbbb".into(),
            wire_proto: 36,
            layout: 1,
            min_loader: 1,
        },
        vec![0xBB; 20_000],
        vec![0xBE; 40_000],
        None,
        None,
    )
    .unwrap()
}

/// A board running X, a release older than Y.
fn board_x() -> BoardManifest {
    BoardManifest {
        proto: 1,
        target: "esp32c6-4mb".into(),
        chip: "esp32c6".into(),
        version: "2026.10.05-1".into(),
        build_id: "2026.10.05-1+aaaaaaaaaaaa".into(),
        wire_proto: 36,
        core_sha256: "aa".repeat(32),
        core_len: 18_000,
        engine_sha256: "ae".repeat(32),
        engine_len: Some(38_000),
        layout: 1,
        loader: 1,
        region_len: 3_375_104,
        state: BoardState::Running,
        refused_build: None,
        transfer: None,
    }
}

fn decide_for(m: BoardManifest) -> Decision {
    decide_with(m, None, false)
}

fn decide_with(m: BoardManifest, user_tier: Option<Tier>, allow_downgrade: bool) -> Decision {
    let build = y();
    decide(
        &BoardView::from_manifest(m),
        &HostFacts {
            build: &build,
            user_tier,
            allow_downgrade,
        },
    )
}

fn heal_of_x() -> Decision {
    Decision::Heal {
        engine_sha: [0xAE; 32],
        build_id: "2026.10.05-1+aaaaaaaaaaaa".into(),
    }
}

#[test]
fn an_older_board_is_offered_the_update() {
    assert_eq!(
        decide_for(board_x()),
        Decision::OfferUpdate {
            from: "2026.10.05-1+aaaaaaaaaaaa".into(),
            to: "2026.10.06-1+bbbbbbbbbbbb".into()
        }
    );
}

#[test]
fn up_to_date_is_decided_by_hashes() {
    let b = y();
    let mut m = board_x();
    m.core_sha256 = sha256_to_hex(&b.core.sha256);
    m.engine_sha256 = sha256_to_hex(&b.engine.sha256);
    // Even under another version string: the hashes decide.
    assert_eq!(decide_for(m), Decision::Nothing);
}

#[test]
fn e1_a_board_waiting_for_its_engine_is_healed() {
    let mut m = board_x();
    m.state = BoardState::NeedsEngine;
    m.engine_len = None;
    assert_eq!(decide_for(m), heal_of_x());
}

#[test]
fn e1_a_heal_needs_no_edit_and_ignores_a_refused_build() {
    let mut m = board_x();
    m.state = BoardState::NeedsEngine;
    m.refused_build = Some(y().build_hash());
    assert_eq!(decide_with(m, Some(Tier::Play), false), heal_of_x());
}

#[test]
fn e2_a_pending_core_transfer_to_the_hosts_build_continues() {
    let mut m = board_x();
    m.state = BoardState::Updating;
    m.engine_len = None;
    m.transfer = Some(TransferView {
        kind: PieceKind::Core,
        done: 8192,
        total: 20_000,
        busy: false,
        build_hash: y().build_hash(),
    });
    assert_eq!(
        decide_for(m),
        Decision::ContinueUpdate {
            to: "2026.10.06-1+bbbbbbbbbbbb".into()
        }
    );
}

#[test]
fn e2_a_pending_transfer_to_another_build_is_healed() {
    let mut m = board_x();
    m.state = BoardState::Updating;
    m.engine_len = None;
    m.transfer = Some(TransferView {
        kind: PieceKind::Core,
        done: 0,
        total: 30_000,
        busy: false,
        build_hash: 0x0BAD_0BAD,
    });
    assert_eq!(decide_for(m), heal_of_x());
}

#[test]
fn e3_a_build_that_failed_its_trial_is_not_offered_again() {
    let mut m = board_x();
    m.refused_build = Some(y().build_hash());
    assert_eq!(
        decide_for(m),
        Decision::RefusedBuild {
            build: y().build_hash()
        }
    );
}

#[test]
fn e5_an_interrupted_update_resumes() {
    // The new core on trial, fetching its engine: a heal of its own build
    // (the host holds it, so it serves its own engine).
    let b = y();
    let mut m = board_x();
    m.state = BoardState::OnTrial;
    m.core_sha256 = sha256_to_hex(&b.core.sha256);
    m.engine_sha256 = sha256_to_hex(&b.engine.sha256);
    m.build_id = b.identity.build_id.clone();
    m.engine_len = None;
    assert_eq!(
        decide_for(m),
        Decision::Heal {
            engine_sha: b.engine.sha256,
            build_id: b.identity.build_id.clone()
        }
    );
}

#[test]
fn e6_a_transfer_another_link_holds_is_busy() {
    let mut m = board_x();
    m.state = BoardState::Updating;
    m.transfer = Some(TransferView {
        kind: PieceKind::Core,
        done: 4096,
        total: 20_000,
        busy: true,
        build_hash: y().build_hash(),
    });
    assert_eq!(
        decide_for(m),
        Decision::Busy {
            done: 4096,
            total: 20_000
        }
    );
}

#[test]
fn e7_no_downgrade_unless_asked() {
    let mut m = board_x();
    m.version = "2026.10.07-1".into();
    assert_eq!(decide_for(m.clone()), Decision::BoardIsNewer);
    assert!(matches!(
        decide_with(m, None, true),
        Decision::OfferUpdate { .. }
    ));
}

#[test]
fn e8_another_chip_layout_an_old_loader_or_no_room_needs_usb() {
    let mut m = board_x();
    m.loader = 0;
    assert_eq!(
        decide_for(m),
        Decision::NeedsUsb {
            why: NeedsUsbWhy::LoaderTooOld { have: 0, need: 1 }
        }
    );
    let mut m = board_x();
    m.region_len = 50_000;
    assert_eq!(
        decide_for(m),
        Decision::NeedsUsb {
            why: NeedsUsbWhy::DoesNotFit {
                need: 60_000,
                room: 50_000
            }
        }
    );
    // A layout or chip this host's code table does not know: the board
    // cannot be updated over a link at all.
    let mut m = board_x();
    m.layout = 2;
    assert_eq!(
        decide_for(m),
        Decision::NeedsUsb {
            why: NeedsUsbWhy::UnknownBoard
        }
    );
    let mut m = board_x();
    m.chip = "esp32s31".into();
    assert_eq!(
        decide_for(m),
        Decision::NeedsUsb {
            why: NeedsUsbWhy::UnknownBoard
        }
    );
}

#[test]
fn e9_a_board_with_no_manifest_needs_usb() {
    let build = y();
    assert_eq!(
        decide(
            &BoardView::absent(),
            &HostFacts {
                build: &build,
                user_tier: None,
                allow_downgrade: false
            }
        ),
        Decision::NeedsUsb {
            why: NeedsUsbWhy::NoUpdateChannel
        }
    );
}

#[test]
fn e10_a_crashing_engine_is_reported_never_auto_healed() {
    let mut m = board_x();
    m.state = BoardState::EngineCrashing;
    assert_eq!(
        decide_for(m),
        Decision::ReportCrashing {
            build_id: "2026.10.05-1+aaaaaaaaaaaa".into()
        }
    );
}

#[test]
fn e12_a_play_only_user_is_not_offered_an_update() {
    assert_eq!(
        decide_with(board_x(), Some(Tier::Play), false),
        Decision::NoUpdateForPlayOnly
    );
    assert!(matches!(
        decide_with(board_x(), Some(Tier::Edit), false),
        Decision::OfferUpdate { .. }
    ));
}

#[test]
fn another_target_is_never_offered_by_default_and_never_parsed() {
    let mut m = board_x();
    m.target = "esp32c6-8mb-variant".into();
    assert_eq!(
        decide_for(m),
        Decision::OtherTarget {
            board_target: "esp32c6-8mb-variant".into()
        }
    );
}

#[test]
fn a_dev_build_against_a_release_is_offered() {
    // M1's comparison: no order between a release and a dev build.
    let mut m = board_x();
    m.version = "626a1b851".into();
    assert!(matches!(decide_for(m), Decision::OfferUpdate { .. }));
}
