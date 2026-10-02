//! `lp-cli emu run --host-link`: the run with this process as the host on
//! the board's USB link (see [`super::link_host`]).
//!
//! One process boots the image, brings the link up, optionally uploads a
//! project and sends requests over it, and keeps hosting to the deadline,
//! writing the decoded console as it goes. That is the shape the walk and
//! the heap ratchet need since the image went onto lp-link: a client that
//! uploads and leaves takes the link's host with it, and the board's log
//! lines after that point would never leave the board.

use std::time::Duration;

use anyhow::{Context, Result, bail};
use lpa_client::LpClient;
use lpc_wire::{ClientMessage, ClientRequest};
use lpfs::LpFsStd;

use super::args::RunArgs;
use super::link_host::{EmuLinkHost, EmuUsbBoard, describe_link_counters, fresh_nonce};
use crate::commands::dev::{collect_project_deploy_files, validation};
use crate::commands::upload::wait::wait_for_project_running;

/// First id of the `--request` requests: far from any the client counts.
const REQUEST_ID_BASE: u64 = 1_000_000;

pub(super) fn run_hosted<B: EmuUsbBoard>(
    board: B,
    describe_boot: String,
    args: &RunArgs,
    micros: u64,
) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(run_hosted_async(board, describe_boot, args, micros))
}

async fn run_hosted_async<B: EmuUsbBoard>(
    board: B,
    describe_boot: String,
    args: &RunArgs,
    micros: u64,
) -> Result<()> {
    eprintln!(
        "emu: {describe_boot}, {} hosted in process (lp-link)",
        board.link_name()
    );
    eprintln!(
        "emu: running for {micros} us of EMULATED time (wall-clock net: {} s)",
        args.wall_timeout_secs
    );
    let nonce = args.link_nonce.unwrap_or_else(fresh_nonce);
    let mut host = EmuLinkHost::new(board, nonce, !args.json_replies)
        .queue_messages(false)
        .wall_timeout(Duration::from_secs(args.wall_timeout_secs));
    if let Some(path) = &args.console {
        let file = std::fs::File::create(path)
            .with_context(|| format!("creating the console transcript {}", path.display()))?;
        host = host.with_console_sink(Box::new(std::io::LineWriter::new(file)));
    }

    let mut failures: Vec<String> = Vec::new();
    let mut ended = None;
    if args.upload.is_some() || !args.request.is_empty() {
        if let Err(error) = converse(&mut host, args, micros).await {
            failures.push(format!("{error:#}"));
        }
    }
    host.set_queue_messages(false);
    let matched = args
        .exit_on
        .as_deref()
        .is_some_and(|needle| host.console().iter().any(|line| line.contains(needle)));
    if matched {
        ended = Some("stopped on --exit-on".to_string());
    } else if failures.is_empty() && host.board.micros() < micros {
        match host.run_until(micros, args.exit_on.as_deref()) {
            Ok(true) => ended = Some("stopped on --exit-on".to_string()),
            Ok(false) => {}
            Err(error) => {
                ended = Some(format!("{error:#}"));
                failures.push(format!("{error:#}"));
            }
        }
    }

    let report = host.board.finish();
    if let Some(path) = &args.console {
        eprintln!(
            "emu: console → {} ({} lines)",
            path.display(),
            host.console().len()
        );
    }
    eprintln!(
        "emu: {} — {} us emulated, {} console lines",
        ended.as_deref().unwrap_or("reached its deadline"),
        host.board.micros(),
        host.console().len(),
    );
    for line in report {
        eprintln!("emu: {line}");
    }
    eprintln!(
        "emu: host link — {}; {} link error(s)",
        describe_link_counters(&host.counters()),
        host.link_errors
    );
    if !failures.is_empty() {
        bail!("{}", failures.join("; "));
    }
    Ok(())
}

/// Wait for the board's hello, then the upload and the requests, in order.
async fn converse<B: EmuUsbBoard>(
    host: &mut EmuLinkHost<B>,
    args: &RunArgs,
    micros: u64,
) -> Result<()> {
    let budget = micros.saturating_sub(host.board.micros());
    if host.wait_for_line("\"hello\":{", budget)?.is_none() {
        bail!("the board never said hello on the link before the deadline");
    }
    eprintln!(
        "emu: link up, hello at {:.3} s emulated",
        host.board_seconds()
    );

    if let Some(dir) = &args.upload {
        let dir = std::env::current_dir()?
            .join(dir)
            .canonicalize()
            .with_context(|| format!("resolving the project directory {}", dir.display()))?;
        let (project_uid, _) = validation::validate_local_project(&dir)?;
        let files = collect_project_deploy_files(&LpFsStd::new(dir.clone()))?;
        host.set_queue_messages(true);
        let mut client = LpClient::new(&mut *host);
        let deploy = client
            .deploy_project_files(&project_uid, files)
            .await
            .map_err(|error| anyhow::anyhow!("{error}"))
            .with_context(|| format!("deploying {}", dir.display()))?;
        wait_for_project_running(&mut client, deploy.into_value(), Duration::from_secs(600))
            .await?;
        drop(client);
        host.set_queue_messages(false);
        eprintln!(
            "emu: {} uploaded and running at {:.3} s emulated",
            dir.display(),
            host.board_seconds()
        );
    }

    for (i, text) in args.request.iter().enumerate() {
        let request: ClientRequest = serde_json::from_str(text)
            .with_context(|| format!("--request `{text}` is not a ClientRequest"))?;
        let id = REQUEST_ID_BASE + i as u64;
        host.send(&ClientMessage { id, msg: request })?;
        let budget = micros.saturating_sub(host.board.micros());
        if host
            .wait_for_line(&format!("M!{{\"id\":{id},"), budget)?
            .is_none()
        {
            bail!("no answer to --request `{text}` before the deadline");
        }
    }
    Ok(())
}
