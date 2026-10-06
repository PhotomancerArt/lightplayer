//! **The host × board simulation** (A-P6, DM26: CI's oracle for the update
//! protocol). `lpa_update::UpdateDriver` drives `lpc_update`'s
//! `BoardSession` on the NOR model through [`world::World`].
//!
//! | Axis | Values |
//! |---|---|
//! | chunk mode | `D` only; `Z` from the packer (some index entries `0`) — `--features pack` |
//! | ahead | 1; 4 |
//! | cut | none; after every Nth request of the whole update; after every flash operation (clean and torn) |
//! | link | steady; dropped every K messages; reordered in-window (recovered by an idle drop); a duplicated chunk |
//! | corruption | none; one bad `D` (→ `N`/`H`, the piece restarts); one bad `Z` (→ asked for raw) |
//! | access | trusted; untrusted + closed + a password; untrusted, no login |
//! | start | running X → Y; engine-less X → heal (then Y); engine-crashing X → report; X with a core transfer pending + a host holding only X → heal (E2) |
//! | intent | `Auto` (the matrix); `Install` and `Reinstall` ([`intents`]) |
//!
//! Every case asserts the end state (the build, a valid engine, or the
//! expected report), that no frozen flash ever lacks a bootable core (the
//! world checks it at every cut), that a cut mid-piece resumes rather than
//! restarts, and that the backup the host kept is the old engine.

mod intents;
mod world;

use lpa_update::{Decision, DriverConfig, Finish, HostBuild, ServeConfig, StopReason};
use lpc_access::{OpenTo, SecretEntry, Tier};
use lpc_update::board::{AccessFacts, LinkTrust, SessionMode};
use lpc_update::code_table::CHUNK;
use lpc_update::hash_rules::engine_sha256;
use lpc_update::testing::{FakeBoard, ModelBuild};
use lpc_update::{BoardState, PieceKind};

use world::{Faults, Outcome, World, host_build, open_access};

const REGION: u32 = 40 * 4096;
const PASSWORD: &[u8] = b"hunter2";

fn x() -> ModelBuild {
    ModelBuild::synthetic("2026.10.05-1", 1, 5 * 4096 + 300, 8 * 4096 + 77)
}

fn y() -> ModelBuild {
    ModelBuild::synthetic("2026.10.06-1", 2, 6 * 4096 + 11, 9 * 4096 + 1000)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Start {
    RunningXToY,
    EnginelessHeal,
    Crashing,
    PendingHeal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Access {
    Trusted,
    Password,
    NoLogin,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    D,
    Z,
}

#[derive(Clone, Debug)]
struct Case {
    start: Start,
    access: Access,
    mode: Mode,
    ahead: u8,
    faults: Faults,
}

/// What a case must end as.
#[derive(Debug, PartialEq, Eq)]
enum Expect {
    /// Running this build with a valid engine, the driver up to date.
    Runs(&'static str),
    /// Stopped for this reason, the board running this build.
    Stops(StopReason, &'static str),
}

fn expect(start: Start, access: Access) -> Expect {
    let y = "Y";
    let x = "X";
    match (start, access) {
        (Start::Crashing, _) => Expect::Stops(
            StopReason::Decision(Decision::ReportCrashing {
                build_id: crate::x().build_id,
            }),
            x,
        ),
        (Start::PendingHeal, _) => Expect::Runs(x),
        // A running engine refuses a core install without a login on its
        // server; a heal never needs one.
        (Start::RunningXToY | Start::EnginelessHeal, Access::NoLogin) => {
            Expect::Stops(StopReason::NeedsEngineLogin, x)
        }
        (Start::RunningXToY | Start::EnginelessHeal, _) => Expect::Runs(y),
    }
}

fn engine_start(core_len: usize) -> u32 {
    (0x8000 + core_len as u32).div_ceil(CHUNK) * CHUNK
}

/// The world for a case, and what it must end as.
fn world(case: &Case, builds: &Builds) -> World {
    let (x, y) = (x(), y());
    let catalog = match case.start {
        Start::Crashing => vec![x.clone().crashing(), y.clone()],
        _ => vec![x.clone(), y.clone()],
    };
    let mut board = FakeBoard::flashed_with(catalog, 0, REGION);
    if case.start == Start::EnginelessHeal {
        board
            .flash
            .flash_image(engine_start(x.core.len()), &[0xFF; CHUNK as usize]);
    }
    let access = match case.access {
        Access::Trusted => open_access(),
        Access::Password | Access::NoLogin => AccessFacts {
            secrets: vec![SecretEntry::from_password(
                "mine",
                Tier::Edit,
                PASSWORD,
                [4; 16],
                16,
            )],
            open: OpenTo::Nobody,
            core_install_follows_open_to: true,
        },
    };
    let trust = match case.access {
        Access::Trusted => LinkTrust::Trusted,
        _ => LinkTrust::Untrusted,
    };
    let config = DriverConfig {
        serve: ServeConfig { ahead: case.ahead },
        ..DriverConfig::default()
    };
    let host = match (case.start, case.mode) {
        (Start::PendingHeal, Mode::D) => builds.x_raw.clone(),
        (Start::PendingHeal, Mode::Z) => builds.x_z.clone(),
        (_, Mode::D) => builds.y_raw.clone(),
        (_, Mode::Z) => builds.y_z.clone(),
    };
    let mut w = World::new(board, access, host, config, trust);
    if case.start == Start::PendingHeal {
        // Another host offered Y over USB; the engine handed over and reset.
        let o = host_build(&y, false).offer().encode();
        w.rig
            .link_up(0, lpc_update::board::LinkId(1), LinkTrust::Trusted);
        w.rig.deliver(0, lpc_update::board::LinkId(1), None, &o);
        assert!(w.rig.reset_pending);
        w.rig.reboot().unwrap();
        assert!(w.rig.session.as_ref().unwrap().transferring());
    }
    // The store holds X's engine (a heal finds it there).
    w.store.insert(engine_sha256(&x.engine), x.engine.clone());
    if case.access == Access::Password {
        w.credentials = vec![lpa_update::Credential::Password(PASSWORD.to_vec())];
        w.engine_tier = Some(Tier::Edit);
    }
    w.faults = case.faults;
    w
}

/// Builds packed once.
struct Builds {
    x_raw: HostBuild,
    y_raw: HostBuild,
    x_z: HostBuild,
    y_z: HostBuild,
}

fn builds() -> Builds {
    let z = cfg!(feature = "pack");
    Builds {
        x_raw: host_build(&x(), false),
        y_raw: host_build(&y(), false),
        x_z: host_build(&x(), z),
        y_z: host_build(&y(), z),
    }
}

/// Run a case and check its end.
fn run(case: &Case, builds: &Builds) -> (Outcome, World) {
    let (out, w) = world(case, builds).run(200_000);
    let name = |b: Option<&ModelBuild>| match b {
        Some(b) if b == &x() || b.core == x().core => "X",
        Some(b) if b == &y() => "Y",
        Some(_) => "?",
        None => "-",
    };
    let (running, engine_valid, mode) = w.running();
    let got = match &out.finish {
        Some(Finish::UpToDate) => Expect::Runs(name(running)),
        Some(Finish::Stopped(r)) => Expect::Stops(r.clone(), name(running)),
        None => panic!("{case:?} did not finish: {out:?}"),
    };
    assert_eq!(got, expect(case.start, case.access), "{case:?}: {out:?}");
    assert!(engine_valid, "{case:?}: the engine is not valid at the end");
    if case.start != Start::Crashing {
        assert_eq!(mode, Some(SessionMode::EngineRunning), "{case:?}");
    }
    if expect(case.start, case.access) == Expect::Runs("Y") {
        // Encoding 1 was really used, and a closed board really logged in.
        if case.mode == Mode::Z {
            assert!(w.driver.served().chunks_encoded > 0, "{case:?}: no Z sent");
        }
        if case.access == Access::Password {
            assert!(out.logins > 0, "{case:?}: no core-side login");
        }
    }
    // The backup the host kept is the old engine (an update backs X up).
    if expect(case.start, case.access) == Expect::Runs("Y") {
        let old = x().engine;
        assert_eq!(
            w.cache.get(&engine_sha256(&old)),
            Some(&old),
            "{case:?}: no backup"
        );
    }
    (out, w)
}

fn modes() -> Vec<Mode> {
    if cfg!(feature = "pack") {
        vec![Mode::D, Mode::Z]
    } else {
        vec![Mode::D]
    }
}

// ---- The matrix -----------------------------------------------------------------------

#[test]
fn every_start_access_mode_and_ahead_converges() {
    let b = builds();
    let mut n = 0;
    for start in [
        Start::RunningXToY,
        Start::EnginelessHeal,
        Start::Crashing,
        Start::PendingHeal,
    ] {
        for access in [Access::Trusted, Access::Password, Access::NoLogin] {
            for mode in modes() {
                for ahead in [1, 4] {
                    let case = Case {
                        start,
                        access,
                        mode,
                        ahead,
                        faults: Faults::default(),
                    };
                    let (out, _) = run(&case, &b);
                    assert_eq!(out.cuts + out.stalls, 0, "{case:?}: {out:?}");
                    n += 1;
                }
            }
        }
    }
    println!("start × access × mode × ahead: {n} cases converged");
}

/// Chunks a clean run of `case` serves.
fn baseline(case: &Case, b: &Builds) -> u32 {
    let clean = Case {
        faults: Faults::default(),
        ..case.clone()
    };
    run(&clean, b).0.chunks_served
}

#[test]
fn a_cut_after_every_request_resumes_and_converges() {
    let b = builds();
    let mut cases = 0;
    let mut resumed = 0;
    for start in [Start::RunningXToY, Start::EnginelessHeal] {
        for mode in modes() {
            for ahead in [1, 4] {
                let base_case = Case {
                    start,
                    access: Access::Trusted,
                    mode,
                    ahead,
                    faults: Faults::default(),
                };
                let base = baseline(&base_case, &b);
                let requests = (x().engine.len() + y().core.len() + y().engine.len())
                    .div_ceil(CHUNK as usize) as u32
                    + 8;
                for n in 1..=requests {
                    let mut case = base_case.clone();
                    case.faults.cut_after_request = Some(n);
                    let (out, _) = run(&case, &b);
                    cases += 1;
                    if let Some((kind, len, marked)) = out.at_cut
                        && marked > 0
                    {
                        let piece = len.div_ceil(CHUNK);
                        assert!(
                            out.chunks_served < base + piece,
                            "{case:?}: served {} after a cut with {marked}/{piece} {kind:?} chunks in (clean: {base})",
                            out.chunks_served
                        );
                        resumed += 1;
                    }
                }
            }
        }
    }
    println!("cut after request N: {cases} cases converged, {resumed} resumed mid-piece");
    assert!(resumed > cases / 3);
}

#[test]
fn a_cut_after_every_flash_operation_converges() {
    let b = builds();
    let mut cases = 0;
    for start in [
        Start::RunningXToY,
        Start::EnginelessHeal,
        Start::PendingHeal,
    ] {
        for mode in modes() {
            // How many operations the clean run takes.
            let clean = Case {
                start,
                access: Access::Trusted,
                mode,
                ahead: 1,
                faults: Faults::default(),
            };
            let w = world(&clean, &b);
            let before = w.rig.board.flash.ops();
            let (_, w) = w.run(200_000);
            let total = w.rig.board.flash.ops() - before;
            for tear in [false, true] {
                for k in 0..total {
                    let mut case = clean.clone();
                    case.faults.cut_after_op = Some(k);
                    case.faults.tear = tear;
                    let (out, _) = run(&case, &b);
                    assert!(out.cuts <= 1, "{case:?}");
                    cases += 1;
                }
            }
        }
    }
    println!("cut after flash op k (clean and torn): {cases} cases converged");
}

#[test]
fn dropped_links_reordering_and_duplicates_converge() {
    let b = builds();
    let mut cases = 0;
    for start in [Start::RunningXToY, Start::EnginelessHeal] {
        for mode in modes() {
            for ahead in [1, 4] {
                for k in [13, 19, 29, 41] {
                    let case = Case {
                        start,
                        access: Access::Password,
                        mode,
                        ahead,
                        faults: Faults {
                            drop_every: Some(k),
                            ..Faults::default()
                        },
                    };
                    let (out, _) = run(&case, &b);
                    assert!(out.link_drops > 0);
                    cases += 1;
                }
                let reorder = Case {
                    start,
                    access: Access::Trusted,
                    mode,
                    ahead,
                    faults: Faults {
                        reorder: true,
                        ..Faults::default()
                    },
                };
                let (out, _) = run(&reorder, &b);
                if ahead > 1 {
                    assert!(
                        out.stalls > 0,
                        "an out-of-order window stalls until the link drops"
                    );
                }
                let dup = Case {
                    faults: Faults {
                        duplicate: Some((PieceKind::Core, 2)),
                        ..Faults::default()
                    },
                    ..reorder.clone()
                };
                run(&dup, &b);
                cases += 2;
            }
        }
    }
    println!("link faults: {cases} cases converged");
}

#[test]
fn a_bad_d_restarts_the_piece_and_a_bad_z_is_asked_for_raw() {
    let b = builds();
    for ahead in [1, 4] {
        let bad_d = Case {
            start: Start::RunningXToY,
            access: Access::Trusted,
            mode: Mode::D,
            ahead,
            faults: Faults {
                corrupt_d: Some((PieceKind::Core, 2)),
                ..Faults::default()
            },
        };
        let base = baseline(&bad_d, &b);
        let (out, _) = run(&bad_d, &b);
        let core = (y().core.len() as u32).div_ceil(CHUNK);
        assert!(
            out.chunks_served >= base + core,
            "N/H restarts the core: {} vs {base}",
            out.chunks_served
        );
        if cfg!(feature = "pack") {
            let first_z = b
                .y_z
                .core
                .encoded
                .as_ref()
                .unwrap()
                .chunks
                .iter()
                .position(|&c| c > 0)
                .unwrap() as u32;
            let bad_z = Case {
                mode: Mode::Z,
                faults: Faults {
                    corrupt_z: Some((PieceKind::Core, first_z)),
                    ..Faults::default()
                },
                ..bad_d.clone()
            };
            let base = baseline(&bad_z, &b);
            let (out, _) = run(&bad_z, &b);
            assert!(out.chunks_served > base, "the bad chunk is sent again, raw");
            assert!(
                out.chunks_served < base + core,
                "the piece does not restart"
            );
        }
    }
}

#[test]
fn a_second_host_takes_over_after_fifteen_seconds_of_quiet() {
    // Host 1 starts a heal on an untrusted link and goes quiet mid-piece;
    // host 2 is told busy, then takes over once 15 s have passed.
    let x = x();
    let mut board = FakeBoard::flashed_with(vec![x.clone()], 0, REGION);
    board
        .flash
        .flash_image(engine_start(x.core.len()), &[0xFF; CHUNK as usize]);
    let mut rig = lpc_update::testing::BoardRig::new(
        board,
        open_access(),
        lpc_update::board::SessionConfig::default(),
    )
    .unwrap();
    let host = host_build(&x, false);
    let (h1, h2) = (lpc_update::board::LinkId(1), lpc_update::board::LinkId(2));
    rig.link_up(0, h1, LinkTrust::Untrusted);
    rig.link_up(0, h2, LinkTrust::Untrusted);
    // One chunk per request, so four rounds leave host 1 mid-transfer.
    let mut serve = lpa_update::ServeSession::new(ServeConfig { ahead: 1 });
    let mut to_board = vec![host.offer().encode()];
    for _ in 0..4 {
        let mut next = Vec::new();
        for m in to_board.drain(..) {
            for o in rig.deliver(10, h1, None, &m) {
                next.extend(serve.on_board(&host, &o.bytes).send);
            }
        }
        to_board = next;
    }
    // Host 2's driver: busy while host 1 is live.
    let mut d2 = lpa_update::UpdateDriver::new(host.clone(), DriverConfig::default());
    d2.link_up(100);
    let busy = drive_one_link(&mut rig, &mut d2, h2, 100);
    assert!(matches!(
        busy,
        Some(Finish::Stopped(StopReason::Decision(Decision::Busy { .. })))
    ));
    // 15 s later host 1 has said nothing: host 2 takes over and finishes.
    let mut d3 = lpa_update::UpdateDriver::new(host, DriverConfig::default());
    d3.link_up(10 + 15_000);
    let done = drive_one_link(&mut rig, &mut d3, h2, 10 + 15_000);
    assert_eq!(done, None, "it served until the board reset");
    assert!(rig.reset_pending);
    rig.reboot().unwrap();
    assert!(rig.board.engine_valid());
    assert_eq!(rig.board.running_build(), Some(&x));
}

/// Drive `d` over `link` until it finishes or the board resets.
fn drive_one_link(
    rig: &mut lpc_update::testing::BoardRig,
    d: &mut lpa_update::UpdateDriver,
    link: lpc_update::board::LinkId,
    now: u64,
) -> Option<Finish> {
    let mut to_board = std::collections::VecDeque::new();
    for _ in 0..10_000 {
        for e in d.take_effects() {
            match e {
                lpa_update::DriverEffect::Send(b) => to_board.push_back(b),
                lpa_update::DriverEffect::Done(f) => return Some(f),
                lpa_update::DriverEffect::Source(
                    lpa_update::decide::SourceEffect::LookUpCache { .. },
                ) => d.source_result(lpa_update::decide::SourceResult::Cache(None)),
                _ => {}
            }
        }
        if rig.reset_pending {
            return None;
        }
        let Some(m) = to_board.pop_front() else {
            return None;
        };
        for o in rig.deliver(now, link, None, &m) {
            d.on_board(now, &o.bytes, &[]);
        }
    }
    panic!("no end");
}

#[test]
fn e13_a_heal_with_no_engine_anywhere_stops_missing() {
    let b = builds();
    let case = Case {
        start: Start::EnginelessHeal,
        access: Access::Trusted,
        mode: Mode::D,
        ahead: 1,
        faults: Faults::default(),
    };
    let mut w = world(&case, &b);
    w.store.clear();
    let (out, w) = w.run(10_000);
    assert_eq!(
        out.finish,
        Some(Finish::Stopped(StopReason::MissingEngine {
            offline: false
        }))
    );
    assert_eq!(w.rig.board.running_build(), Some(&x()));
    assert!(!w.rig.board.engine_valid(), "waiting, dark red");
    assert_eq!(w.driver.board().state(), Some(BoardState::NeedsEngine));
}
