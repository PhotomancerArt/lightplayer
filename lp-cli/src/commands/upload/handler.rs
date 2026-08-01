//! Upload command handler
//!
//! Pushes a project to a host (e.g. serial device). By default waits for
//! evidence the newly deployed project is running before exiting (see
//! `wait::wait_for_project_running` and
//! `docs/defects/2026-07-30-deploy-compiles-previous-upload.md`); pass
//! `--no-wait` to restore the old fire-and-forget behaviour.

use std::time::Duration;

use anyhow::{Context, Result};
use lpa_client::LpClient;
use lpfs::LpFsStd;

use crate::client::cli_connect::{cli_connect, stderr_device_events};
use crate::commands::dev::{collect_project_deploy_files, validation};
use lpa_client::HostSpecifier;

use super::args::UploadArgs;
use super::wait::wait_for_project_running;

/// Handle the upload command
pub fn handle_upload(args: UploadArgs) -> Result<()> {
    // Device connections are single-actor (`!Send`): current-thread runtime
    // + LocalSet, matching the DeviceSession world.
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let local = tokio::task::LocalSet::new();
    runtime.block_on(local.run_until(handle_upload_async(args)))
}

async fn handle_upload_async(args: UploadArgs) -> Result<()> {
    let dir = std::env::current_dir()
        .with_context(|| "Failed to get current directory")?
        .join(&args.dir)
        .canonicalize()
        .with_context(|| {
            format!(
                "Failed to resolve project directory: {}",
                args.dir.display()
            )
        })?;

    let (project_uid, _project_name) = validation::validate_local_project(&dir)?;

    let host_spec = HostSpecifier::parse(&args.host).with_context(|| {
        format!(
            "Failed to parse host specifier: {}. Examples: serial:auto, ws://localhost:2812/",
            args.host
        )
    })?;

    let host_spec_str = format!("{host_spec:?}");

    let connection = cli_connect(host_spec, stderr_device_events(false))
        .await
        .context("Failed to connect to server")?;
    let mut client = LpClient::new(connection.client_io());

    let local_fs = LpFsStd::new(dir);
    let files = collect_project_deploy_files(&local_fs)?;
    let deploy = client
        .deploy_project_files(&project_uid, files)
        .await
        .map_err(|error| anyhow::anyhow!("{error}"))
        .with_context(|| format!("Failed to deploy project to server (host: {host_spec_str})"))?;
    let handle = deploy.into_value();

    // The wait (or its absence) happens on this same connection — the S3
    // serial port is exclusive, so a monitor cannot attach separately.
    let wait_result = if args.no_wait {
        Ok(())
    } else {
        wait_for_project_running(
            &mut client,
            handle,
            Duration::from_secs(args.wait_timeout_secs),
        )
        .await
    };

    drop(client);
    connection.close().await;

    wait_result?;

    if args.no_wait {
        println!("Project uploaded and loaded successfully.");
    } else {
        println!("Project uploaded and running.");
    }
    Ok(())
}
