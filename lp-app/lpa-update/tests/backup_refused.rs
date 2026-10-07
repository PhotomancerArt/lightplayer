//! The read-back a backup needs can be refused: a running engine on a link
//! it does not trust answers `G` with `N`/`A` until its server's login
//! (channel 1) grants play.
//!
//! - On a link the board never answered, the driver stops
//!   `NeedsEngineLogin` at once, rather than wait for data that will never
//!   come (found on the emulated C6 with `fixture-usb-untrusted`, OTA plan
//!   Part B, U8).
//! - On a link that came back after a drop mid-backup, the refusal is a race
//!   with the caller's login there (Studio over Bluetooth, 2026-10-07 desk
//!   run s1: four `G`s refused 0.2 s after the reconnect, and the update
//!   ended `NeedsEngineLogin` at "Backing up… 0%"). The driver keeps the
//!   backup, asks again, and resumes once the board answers — or stops
//!   after `ENGINE_LOGIN_WAIT_MS`.

use lpa_update::decide::{SourceEffect, SourceResult, StoreAnswer};
use lpa_update::drive::update_driver::{
    ENGINE_LOGIN_RETRY_MS, ENGINE_LOGIN_WAIT_MS, READ_BACK_STALE_MS,
};
use lpa_update::{
    BLE_READ_BACK_PIECE, DriverConfig, DriverEffect, Finish, HostBuild, HostIdentity, ServeConfig,
    Stage, StopReason, UpdateDriver, UpdateIntent,
};
use lpc_update::hash_rules::engine_sha256;
use lpc_update::{
    BoardManifest, BoardState, CHUNK, ChunkEncoding, HostMessage, PieceKind, Refusal, encode_chunk,
    sha256_to_hex,
};

#[test]
fn a_refused_read_back_stops_the_driver_with_needs_engine_login() {
    let mut driver = UpdateDriver::new(
        y(),
        DriverConfig {
            intent: UpdateIntent::Install {
                allow_downgrade: false,
            },
            ..DriverConfig::default()
        },
    );
    driver.link_up(0);
    let mut m = b"M".to_vec();
    m.extend_from_slice(&board_x().to_json());
    driver.on_board(1, &m, &[]);
    // The engine source: no cache, no store, then the read-back.
    for _ in 0..4 {
        for effect in driver.take_effects() {
            match effect {
                DriverEffect::Source(SourceEffect::LookUpCache { .. }) => {
                    driver.source_result(SourceResult::Cache(None));
                }
                DriverEffect::Source(SourceEffect::FetchFromStore { .. }) => {
                    driver.source_result(SourceResult::Store(StoreAnswer::Offline));
                }
                _ => {}
            }
        }
    }
    driver.on_board(2, &Refusal::Access.encode(), &[]);
    let done = driver.take_effects().into_iter().find_map(|e| match e {
        DriverEffect::Done(f) => Some(f),
        _ => None,
    });
    assert_eq!(done, Some(Finish::Stopped(StopReason::NeedsEngineLogin)));
    assert!(driver.done());
}

#[test]
fn a_backup_cut_by_a_drop_waits_for_the_engine_login_on_the_new_link_and_resumes() {
    let engine: Vec<u8> = (0..10 * CHUNK).map(|i| (i * 7 + i / 4096) as u8).collect();
    let board = board_x_with(&engine);
    let (mut driver, first) = driver_backing_up(&board);
    // Over Bluetooth the backup reads in pieces that fit the radio link's
    // send ring, four chunks' worth outstanding.
    let p = BLE_READ_BACK_PIECE;
    let ahead = ServeConfig::BLE.read_back_ahead();
    let pieces = (engine.len() as u32).div_ceil(p);
    assert_eq!(ahead, 17);
    assert_eq!(gets(&first), offsets(0, ahead, p));
    // Two pieces in on the first link.
    driver.on_board(10, &d(&engine, 0, p), &[]);
    driver.on_board(11, &d(&engine, p, p), &[]);
    driver.take_effects();

    // The link drops; a new one comes up and the board answers `M` before
    // the caller's login on it has landed: every `G` is refused `N`/`A`.
    driver.link_down(20);
    driver.link_up(2_000);
    assert_eq!(queries(&driver.take_effects()), 1);
    driver.on_board(2_010, &manifest(&board), &[]);
    let resumed = gets(&driver.take_effects());
    assert_eq!(resumed, offsets(2, ahead, p));
    for t in 0..u64::from(ahead) {
        driver.on_board(2_200 + t, &Refusal::Access.encode(), &[]);
    }
    assert!(
        ended(&driver.take_effects()).is_none(),
        "a refused read-back on a new link waits for the engine's login"
    );
    assert!(!driver.done());
    // Nothing is asked before the retry is due…
    driver.tick(2_200 + ENGINE_LOGIN_RETRY_MS - 1);
    assert!(driver.take_effects().is_empty());
    // …then `Q` again; the login has landed now, so `M` resumes the backup
    // from the first missing piece.
    driver.tick(2_200 + ENGINE_LOGIN_RETRY_MS);
    assert_eq!(queries(&driver.take_effects()), 1);
    driver.on_board(3_300, &manifest(&board), &[]);
    let again = gets(&driver.take_effects());
    assert_eq!(again, offsets(2, ahead, p));
    let mut offered = false;
    let mut last_progress = 0;
    for idx in 2..pieces {
        driver.on_board(3_400 + u64::from(idx), &d(&engine, idx * p, p), &[]);
        for effect in driver.take_effects() {
            match effect {
                DriverEffect::Progress {
                    stage: Stage::BackingUp,
                    done,
                    ..
                } => last_progress = done,
                DriverEffect::Send(bytes) if bytes.first() == Some(&b'O') => offered = true,
                DriverEffect::Done(finish) => panic!("ended early: {finish:?}"),
                _ => {}
            }
        }
    }
    assert_eq!(
        last_progress,
        (pieces - 1) * p,
        "the backup counted up to its last piece"
    );
    assert!(offered, "the backup finished and the update was offered");
}

#[test]
fn a_read_back_piece_lost_with_the_link_up_is_asked_again() {
    let engine: Vec<u8> = (0..3 * CHUNK).map(|i| (i * 5) as u8).collect();
    let board = board_x_with(&engine);
    let (mut driver, first) = driver_backing_up(&board);
    let p = BLE_READ_BACK_PIECE;
    let pieces = (engine.len() as u32).div_ceil(p);
    let asked = gets(&first);
    assert_eq!(asked[0], 0);
    // Piece 0's answer never comes; every other piece does.
    for idx in 1..pieces {
        driver.on_board(100, &d(&engine, idx * p, p), &[]);
    }
    driver.take_effects();
    // Before the piece is stale nothing is asked again…
    driver.tick(READ_BACK_STALE_MS - 1);
    assert!(gets(&driver.take_effects()).is_empty());
    // …then it is, and its answer finishes the backup.
    driver.tick(READ_BACK_STALE_MS);
    assert_eq!(gets(&driver.take_effects()), [0]);
    driver.on_board(READ_BACK_STALE_MS + 10, &d(&engine, 0, p), &[]);
    let offered = driver
        .take_effects()
        .iter()
        .any(|e| matches!(e, DriverEffect::Send(bytes) if bytes.first() == Some(&b'O')));
    assert!(offered, "the backup finished and the update was offered");
}

#[test]
fn a_wait_for_the_engine_login_gives_up_and_stops_needs_engine_login() {
    let engine: Vec<u8> = (0..6 * CHUNK).map(|i| (i * 3) as u8).collect();
    let board = board_x_with(&engine);
    let (mut driver, _) = driver_backing_up(&board);
    driver.on_board(10, &d(&engine, 0, BLE_READ_BACK_PIECE), &[]);
    driver.link_down(20);
    driver.link_up(1_000);
    driver.on_board(1_010, &manifest(&board), &[]);
    driver.take_effects();
    let start = 1_020;
    driver.on_board(start, &Refusal::Access.encode(), &[]);
    assert!(ended(&driver.take_effects()).is_none());
    // Every retry is refused too: no login ever lands on this link.
    let mut refused_at = start;
    while refused_at <= start + ENGINE_LOGIN_WAIT_MS {
        let now = refused_at + ENGINE_LOGIN_RETRY_MS;
        driver.tick(now);
        assert_eq!(queries(&driver.take_effects()), 1, "one Q per retry");
        driver.on_board(now + 5, &manifest(&board), &[]);
        driver.take_effects();
        refused_at = now + 10;
        driver.on_board(refused_at, &Refusal::Access.encode(), &[]);
        if let Some(finish) = ended(&driver.take_effects()) {
            assert_eq!(finish, Finish::Stopped(StopReason::NeedsEngineLogin));
            assert!(
                refused_at >= start + ENGINE_LOGIN_WAIT_MS,
                "gave up early at {refused_at}"
            );
            assert!(driver.done());
            return;
        }
    }
    panic!("the wait never gave up");
}

/// A driver told to install Y on `board`, its engine in no cache or store,
/// and the read-back's first effects (the `G`s).
fn driver_backing_up(board: &BoardManifest) -> (UpdateDriver, Vec<DriverEffect>) {
    let mut driver = UpdateDriver::new(
        y(),
        DriverConfig {
            serve: ServeConfig::BLE,
            intent: UpdateIntent::Install {
                allow_downgrade: false,
            },
            ..DriverConfig::default()
        },
    );
    driver.link_up(0);
    driver.take_effects();
    driver.on_board(0, &manifest(board), &[]);
    loop {
        let effects = driver.take_effects();
        let mut sourced = false;
        for effect in &effects {
            match effect {
                DriverEffect::Source(SourceEffect::LookUpCache { .. }) => {
                    driver.source_result(SourceResult::Cache(None));
                    sourced = true;
                }
                DriverEffect::Source(SourceEffect::FetchFromStore { .. }) => {
                    driver.source_result(SourceResult::Store(StoreAnswer::NotFound));
                    sourced = true;
                }
                _ => {}
            }
        }
        if !sourced {
            return (driver, effects);
        }
    }
}

fn gets(effects: &[DriverEffect]) -> Vec<u32> {
    effects
        .iter()
        .filter_map(|e| match e {
            DriverEffect::Send(bytes) => match HostMessage::decode(bytes) {
                Ok(HostMessage::ReadBack(g)) => Some(g.off),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

fn queries(effects: &[DriverEffect]) -> usize {
    effects
        .iter()
        .filter(|e| matches!(e, DriverEffect::Send(bytes) if bytes.first() == Some(&b'Q')))
        .count()
}

fn ended(effects: &[DriverEffect]) -> Option<Finish> {
    effects.iter().find_map(|e| match e {
        DriverEffect::Done(f) => Some(f.clone()),
        _ => None,
    })
}

fn offsets(from: u32, n: u8, piece: u32) -> Vec<u32> {
    (from..from + u32::from(n)).map(|i| i * piece).collect()
}

fn d(engine: &[u8], off: u32, piece: u32) -> Vec<u8> {
    let end = (off + piece).min(engine.len() as u32) as usize;
    encode_chunk(
        ChunkEncoding::Raw,
        PieceKind::Engine,
        off,
        &engine[off as usize..end],
    )
}

fn manifest(board: &BoardManifest) -> Vec<u8> {
    let mut m = b"M".to_vec();
    m.extend_from_slice(&board.to_json());
    m
}

fn board_x_with(engine: &[u8]) -> BoardManifest {
    BoardManifest {
        engine_sha256: sha256_to_hex(&engine_sha256(engine)),
        engine_len: Some(engine.len() as u32),
        ..board_x()
    }
}

fn y() -> HostBuild {
    HostBuild::from_parts(
        HostIdentity {
            target: "esp32c6-4mb".into(),
            chip: "esp32c6".into(),
            version: "2026.10.06-1".into(),
            build_id: "2026.10.06-1+bbbbbbbbbbbb".into(),
            wire_proto: 37,
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

fn board_x() -> BoardManifest {
    BoardManifest {
        proto: 1,
        target: "esp32c6-4mb".into(),
        chip: "esp32c6".into(),
        version: "2026.10.05-1".into(),
        build_id: "2026.10.05-1+aaaaaaaaaaaa".into(),
        wire_proto: 37,
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
