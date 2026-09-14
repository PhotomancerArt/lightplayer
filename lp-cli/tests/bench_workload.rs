//! The generated bench workload is a project a server can actually run.
//!
//! The soft-limit bench measures where a board dies. That number is only
//! meaningful if every *surviving* step really rendered — so a workload that
//! fails to load would read as "the board died" and silently poison the
//! measurement. This test proves the generator's output loads and renders
//! before any hardware is involved, using the same `handle_upload` path the
//! bench drives against `HostSpecifier::Local`: an in-process `fw-host`
//! runtime ticking a real `LpServer` (see `tests/upload_wait.rs`).

use lp_cli::commands::hardware::bench::workload::{BenchWorkload, workload_endpoint};
use lp_cli::commands::upload::{UploadArgs, handle_upload};
use lpc_hardware::HardwareManifestFile;
use lpc_model::HwEndpointSpec;
use tempfile::TempDir;

#[test]
fn generated_workload_reaches_running_on_a_host_server() {
    let dir = TempDir::new().expect("tempdir");
    let project_dir = dir.path().join("bench-workload");
    BenchWorkload::new(30, endpoint())
        .write_to(&project_dir)
        .expect("workload written");

    let result = handle_upload(UploadArgs {
        dir: project_dir,
        host: "local".to_string(),
        no_wait: false,
        wait_timeout_secs: 30,
    });

    assert!(
        result.is_ok(),
        "the generated bench workload must load and render on a host server \
         (a workload that cannot run would read as a dead board): {result:?}"
    );
}

/// The workload scales without breaking: a count large enough to matter for
/// the metric still loads and renders.
#[test]
fn a_large_strip_still_runs() {
    let dir = TempDir::new().expect("tempdir");
    let project_dir = dir.path().join("bench-workload-400");
    BenchWorkload::new(400, endpoint())
        .write_to(&project_dir)
        .expect("workload written");

    let result = handle_upload(UploadArgs {
        dir: project_dir,
        host: "local".to_string(),
        no_wait: false,
        wait_timeout_secs: 30,
    });

    assert!(result.is_ok(), "400-LED workload should run: {result:?}");
}

/// The endpoint the generator picks for a real bench board is one a host
/// server accepts, so the smoke test above is not exercising a special case.
fn endpoint() -> HwEndpointSpec {
    let manifest = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("workspace dir")
            .join("lp-core/lpc-hardware/boards/seeed/xiao-esp32-c6.json"),
    )
    .expect("board manifest");
    workload_endpoint(&HardwareManifestFile::read_json(&manifest).expect("board manifest parses"))
        .expect("bench board has a data pin")
}
