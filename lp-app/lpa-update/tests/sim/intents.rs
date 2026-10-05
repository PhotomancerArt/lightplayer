//! **The person's intents** against the real board session (DS6): what an
//! `Install` or a `Reinstall` does that `Auto` never would, and what no
//! intent gets past.
//!
//! | Intent | Board | Ends |
//! |---|---|---|
//! | `Reinstall` | X crashing; the host holds X | X's engine written once, still crashing (the model's crash is the build's) → `ReportCrashing` |
//! | `Reinstall` | X crashing; X's engine in the cache or the store | the same, the engine from there |
//! | `Reinstall` | X crashing; X's engine nowhere | `MissingEngine` |
//! | `Install` | X engine-less, its engine nowhere (E13) | Y, core then engine; and with a cut after every flash op |
//! | `Install` | X crashing | Y |
//! | `Install { allow_downgrade }` | running Y, the host X | X with it; `BoardIsNewer` without |
//! | `Install` | X that refused Y | `RefusedBuild`, nothing sent but `Q` |

use std::collections::VecDeque;

use lpa_update::{
    Decision, DriverConfig, Finish, HostBuild, ServeConfig, ServeSession, StopReason, UpdateIntent,
};
use lpc_update::board::{LinkId, LinkTrust, SessionConfig, SessionMode};
use lpc_update::code_table::CHUNK;
use lpc_update::hash_rules::engine_sha256;
use lpc_update::testing::{BoardRig, FakeBoard, ModelBuild};

use crate::world::{Outcome, World, host_build, open_access};
use crate::{REGION, engine_start, x, y};

const INSTALL: UpdateIntent = UpdateIntent::Install {
    allow_downgrade: false,
};

#[test]
fn reinstall_writes_the_engine_once_and_a_crash_in_the_build_still_reports() {
    // The model's crash is intrinsic to the build (`ModelBuild::crashing`):
    // the same engine, written again, crashes again. The board takes the
    // engine install by hashes while engine-crashing; the driver writes it
    // once, and stops when the board comes back crashing.
    let (out, w) = run(
        crashing_x(),
        host_build(&x(), false),
        UpdateIntent::Reinstall,
        |_| {},
    );
    assert_reinstalled_once(&out, &w);
    assert!(w.cache.is_empty(), "the host's own engine: nothing sourced");
}

#[test]
fn reinstall_finds_the_boards_engine_in_the_cache_or_the_store() {
    let engine = x().engine;
    for in_cache in [true, false] {
        let (out, w) = run(
            crashing_x(),
            host_build(&y(), false),
            UpdateIntent::Reinstall,
            |w| {
                let at = if in_cache { &mut w.cache } else { &mut w.store };
                at.insert(engine_sha256(&engine), engine.clone());
            },
        );
        assert_reinstalled_once(&out, &w);
        // From the store, the engine is kept in the cache.
        assert_eq!(w.cache.get(&engine_sha256(&engine)), Some(&engine));
    }
}

#[test]
fn reinstall_with_the_engine_nowhere_is_missing() {
    let (out, w) = run(
        crashing_x(),
        host_build(&y(), false),
        UpdateIntent::Reinstall,
        |_| {},
    );
    assert_eq!(
        out.finish,
        Some(Finish::Stopped(StopReason::MissingEngine {
            offline: false
        }))
    );
    assert_eq!(out.chunks_served, 0);
    assert_eq!(w.rig.boots, 1, "never reset");
    assert_eq!(w.rig.board.running_build(), Some(&x().crashing()));
    assert!(
        w.rig.board.engine_valid(),
        "the crashing engine is untouched"
    );
}

#[test]
fn e13_install_puts_the_hosts_build_on_a_board_waiting_for_an_engine_nobody_has() {
    // Auto would heal X, and miss (E13's `MissingEngine`). Install is the
    // person choosing Y instead: a core install, then Y's own engine.
    let (out, w) = run(engineless_x(), host_build(&y(), false), INSTALL, |_| {});
    assert_runs(&out, &w, &y());
    assert!(out.chunks_served > 0);
    assert!(
        w.cache.is_empty(),
        "no engine on the board: nothing backed up"
    );
}

#[test]
fn e13_install_with_a_cut_after_every_flash_operation_converges() {
    let host = host_build(&y(), false);
    let w = world(engineless_x(), host.clone(), INSTALL);
    let before = w.rig.board.flash.ops();
    let (_, w) = w.run(200_000);
    let total = w.rig.board.flash.ops() - before;
    let mut cases = 0;
    for tear in [false, true] {
        for k in 0..total {
            let mut w = world(engineless_x(), host.clone(), INSTALL);
            w.faults.cut_after_op = Some(k);
            w.faults.tear = tear;
            let (out, w) = w.run(200_000);
            assert!(out.cuts <= 1, "k={k} tear={tear}: {out:?}");
            assert_runs(&out, &w, &y());
            cases += 1;
        }
    }
    println!(
        "Install on an engine-less board, cut after flash op k (clean and torn): {cases} cases converged"
    );
}

#[test]
fn install_passes_a_crashing_engine() {
    let (out, w) = run(crashing_x(), host_build(&y(), false), INSTALL, |_| {});
    assert_runs(&out, &w, &y());
    let old = x().engine;
    assert_eq!(
        w.cache.get(&engine_sha256(&old)),
        Some(&old),
        "the crashing engine was backed up first (D2)"
    );
}

#[test]
fn install_with_allow_downgrade_puts_an_older_build_on_a_newer_board() {
    let newer = FakeBoard::flashed_with(vec![x(), y()], 1, REGION);
    let downgrade = UpdateIntent::Install {
        allow_downgrade: true,
    };
    let (out, w) = run(newer.clone(), host_build(&x(), false), downgrade, |_| {});
    assert_runs(&out, &w, &x());
    let old = y().engine;
    assert_eq!(
        w.cache.get(&engine_sha256(&old)),
        Some(&old),
        "Y's engine backed up"
    );

    let (out, w) = run(newer, host_build(&x(), false), INSTALL, |_| {});
    assert_eq!(
        out.finish,
        Some(Finish::Stopped(StopReason::Decision(
            Decision::BoardIsNewer
        )))
    );
    assert_eq!(w.rig.board.running_build(), Some(&y()));
    assert_eq!(w.host_sent, b"Q");
}

#[test]
fn install_of_a_refused_build_stops_with_nothing_sent_but_q() {
    // QY2: the board refuses a build whose trial failed, forever; there is
    // no "try again".
    let board = x_that_refused_y();
    let ops = board.flash.ops();
    let (out, w) = run(board, host_build(&y(), false), INSTALL, |_| {});
    assert_eq!(
        out.finish,
        Some(Finish::Stopped(StopReason::Decision(
            Decision::RefusedBuild {
                build: y().build_hash()
            }
        )))
    );
    assert_eq!(w.host_sent, b"Q");
    assert_eq!(w.rig.board.flash.ops(), ops, "nothing written");
    assert_eq!(w.rig.board.running_build(), Some(&x()));
}

// ---- Helpers ------------------------------------------------------------------------

fn world(board: FakeBoard, host: HostBuild, intent: UpdateIntent) -> World {
    let config = DriverConfig {
        intent,
        ..DriverConfig::default()
    };
    World::new(board, open_access(), host, config, LinkTrust::Trusted)
}

fn run(
    board: FakeBoard,
    host: HostBuild,
    intent: UpdateIntent,
    setup: impl FnOnce(&mut World),
) -> (Outcome, World) {
    let mut w = world(board, host, intent);
    setup(&mut w);
    w.run(200_000)
}

/// Ends up to date, running `build` on a valid engine.
fn assert_runs(out: &Outcome, w: &World, build: &ModelBuild) {
    assert_eq!(out.finish, Some(Finish::UpToDate), "{out:?}");
    assert_eq!(w.rig.board.running_build(), Some(build));
    assert!(w.rig.board.engine_valid());
    assert_eq!(w.rig.mode(), Some(SessionMode::EngineRunning));
}

/// One engine install served, one reset, and the board back on its own
/// build, still crashing: the driver stopped instead of writing it again.
fn assert_reinstalled_once(out: &Outcome, w: &World) {
    assert_eq!(
        out.finish,
        Some(Finish::Stopped(StopReason::Decision(
            Decision::ReportCrashing {
                build_id: x().build_id
            }
        )))
    );
    let engine_chunks = (x().engine.len() as u32).div_ceil(CHUNK);
    assert_eq!(out.chunks_served, engine_chunks, "the engine, once");
    assert_eq!(w.rig.boots, 2, "one reset: the reinstall's");
    assert_eq!(w.rig.board.running_build(), Some(&x().crashing()));
    assert!(w.rig.board.engine_valid());
    assert_eq!(w.rig.mode(), Some(SessionMode::CoreOnly));
    assert!(
        !w.host_sent.contains(&b'G'),
        "no backup of an engine-only install"
    );
}

/// X whose engine crashes whenever it runs (E10).
fn crashing_x() -> FakeBoard {
    FakeBoard::flashed_with(vec![x().crashing(), y()], 0, REGION)
}

/// X with no engine: core-only, waiting for it (E1/E13).
fn engineless_x() -> FakeBoard {
    let mut board = FakeBoard::flashed_with(vec![x(), y()], 0, REGION);
    board
        .flash
        .flash_image(engine_start(x().core.len()), &[0xFF; CHUNK as usize]);
    board
}

/// X that took Y's core, whose trial crashed warm and rolled back (E3), with
/// X's engine put back: running X, `refusedBuild` = Y.
fn x_that_refused_y() -> FakeBoard {
    let board = FakeBoard::flashed_with(vec![x(), y()], 0, REGION);
    let mut rig = BoardRig::new(board, open_access(), SessionConfig::default()).unwrap();
    let host = host_build(&y(), false);
    // Two legs: the running engine hands over, then core-only takes the core.
    for leg in 1..=2 {
        let link = LinkId(leg);
        rig.link_up(0, link, LinkTrust::Trusted);
        let mut serve = ServeSession::new(ServeConfig::USB);
        let mut to_board = VecDeque::from([host.offer().encode()]);
        while !rig.reset_pending {
            let m = to_board
                .pop_front()
                .expect("the board asks until it resets");
            for o in rig.deliver(0, link, None, &m) {
                to_board.extend(serve.on_board(&host, &o.bytes).send);
            }
        }
        rig.reboot().unwrap();
    }
    assert_eq!(rig.board.running_build(), Some(&y()), "Y on trial");
    rig.board.fail_trial();
    rig.reboot().unwrap();
    rig.board
        .flash
        .flash_image(engine_start(x().core.len()), &x().engine);
    rig.reboot().unwrap();
    assert_eq!(rig.mode(), Some(SessionMode::EngineRunning));
    let m = rig.session.as_ref().unwrap().manifest(0);
    assert_eq!(m.refused_build, Some(y().build_hash()));
    rig.board
}
