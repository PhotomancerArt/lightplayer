//! Upload command handler
//!
//! Pushes a project to a host (e.g. serial device) and exits. Non-interactive.

use anyhow::{Context, Result};
use lpfs::{LpFs, LpFsStd};
use std::sync::Arc;

use crate::client::{LpClient, client_connect};
use crate::commands::dev::{push_project_async, validation};
use lp_client::HostSpecifier;

use super::args::UploadArgs;

/// Handle the upload command
pub fn handle_upload(args: UploadArgs) -> Result<()> {
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(handle_upload_async(args))
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

    let transport = client_connect(host_spec).context("Failed to connect to server")?;
    let shared_transport = Arc::new(tokio::sync::Mutex::new(transport));
    let client = Arc::new(LpClient::new_shared(Arc::clone(&shared_transport)));

    let local_fs: Arc<dyn LpFs> = Arc::new(LpFsStd::new(dir));

    if let Err(e) = client.stop_all_projects().await {
        eprintln!("Warning: Failed to stop all projects: {e}");
        eprintln!("Continuing with project push...");
    }

    push_project_async(&client, &*local_fs, &project_uid)
        .await
        .with_context(|| format!("Failed to push project to server (host: {host_spec_str})"))?;

    let project_path = format!("projects/{project_uid}");
    client
        .project_load(&project_path)
        .await
        .with_context(|| format!("Failed to load project on server: {project_path}"))?;

    println!("Project uploaded and loaded successfully.");
    Ok(())
}
