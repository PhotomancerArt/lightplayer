//! `lp-cli hardware stamp`: write a board manifest to a device's
//! `/hardware.json`, the same write Studio's flash ladder makes.
//!
//! The firmware reads `/hardware.json` at boot and prefers it to its
//! compiled-in manifest, so a board stamped by an older Studio keeps the
//! older pin map — capabilities included — across firmware updates. This
//! re-stamps it from the command line. It takes effect on the next boot.

use anyhow::{Context, Result};
use lpa_client::{HostSpecifier, LpClient, MANIFEST_CHUNK_BYTES, write_file_in_chunks};
use lpc_hardware::HardwareManifestFile;
use lpc_model::AsLpPath;

use crate::client::cli_connect::{cli_connect, stderr_device_events};

use super::args::StampArgs;

const DEVICE_HARDWARE_MANIFEST_PATH: &str = "/hardware.json";

pub fn handle_stamp(args: StampArgs) -> Result<()> {
    // Device connections are single-actor (`!Send`), as in `upload`.
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let local = tokio::task::LocalSet::new();
    runtime.block_on(local.run_until(handle_stamp_async(args)))
}

async fn handle_stamp_async(args: StampArgs) -> Result<()> {
    let json = std::fs::read_to_string(&args.manifest)
        .with_context(|| format!("Failed to read {}", args.manifest.display()))?;
    // Refuse anything the board's loader would refuse: a board that cannot
    // read its manifest falls back to the compiled-in one with a warning,
    // which is not what anyone stamping one meant.
    let manifest = HardwareManifestFile::read_json(&json)
        .and_then(|file| file.to_manifest())
        .map_err(|error| anyhow::anyhow!("{}: {error}", args.manifest.display()))?;

    let host_spec = HostSpecifier::parse(&args.host).with_context(|| {
        format!(
            "Failed to parse host specifier: {}. Examples: serial:auto, serial:/dev/cu.usbmodem2101",
            args.host
        )
    })?;
    let connection = cli_connect(host_spec, stderr_device_events(false))
        .await
        .context("Failed to connect to the device")?;
    let mut client = LpClient::new(connection.client_io());
    let mut report = |label: String, _percent: Option<u8>| eprintln!("{label}");
    let result = write_file_in_chunks(
        &mut client,
        DEVICE_HARDWARE_MANIFEST_PATH.as_path(),
        json.as_bytes(),
        MANIFEST_CHUNK_BYTES,
        &mut report,
    )
    .await;
    drop(client);
    connection.close().await;
    result.map_err(|error| anyhow::anyhow!("{error}"))?;

    println!(
        "Stamped {} ({}) onto {DEVICE_HARDWARE_MANIFEST_PATH}. Reset the board to use it.",
        manifest.board_id(),
        manifest.board_name()
    );
    Ok(())
}
