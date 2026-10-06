//! The read-back a backup needs can be refused: a running engine on a link
//! it does not trust answers `G` with `N`/`A` until its server's login
//! (channel 1) grants play. The driver stops `NeedsEngineLogin` then,
//! rather than wait for data that will never come (found on the emulated
//! C6 with `fixture-usb-untrusted`, OTA plan Part B, U8).

use lpa_update::decide::{SourceEffect, SourceResult, StoreAnswer};
use lpa_update::{
    DriverConfig, DriverEffect, Finish, HostBuild, HostIdentity, StopReason, UpdateDriver,
    UpdateIntent,
};
use lpc_update::{BoardManifest, BoardState, Refusal};

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
