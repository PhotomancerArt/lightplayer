//! `lp-cli firmware install --release <version|previous|latest>` — put any
//! published release's firmware on a board over USB.
//!
//! Resolves the release, downloads and verifies its package against
//! lightplayer.app's public firmware lookup ([`package_fetch`]), leases the
//! board on the desk's board bench when one is present, and writes it with
//! the same layout-aware flasher `lp-cli hardware lpfs migrate` uses
//! ([`flash`]).
//!
//! No wire/protocol compatibility games here either: a board runs one
//! firmware at a time, and this is how to put a *different* one on it on
//! purpose — rolling back to `previous`, or picking up a release that has
//! not reached this board through its own update path yet.

mod flash;
mod package_fetch;
mod release_catalog_source;
mod release_resolve;

use anyhow::{Context, Result, bail};

use super::args::InstallArgs;
use super::distribution_manifest::DistributionManifest;
use crate::client::board_bench;
use crate::commands::fwcheck::port;
use package_fetch::{LightplayerLookup, ReleaseFile, fetch_package};
use release_catalog_source::default_catalog;
use release_resolve::{ReleaseRequest, previous_version};

pub fn handle_install(args: InstallArgs) -> Result<()> {
    let request = ReleaseRequest::parse(&args.release)?;

    // `--as`, else `$BOARD_HOLDER` (the board bench's own fallback, read
    // from this process's environment by the `board` subprocess itself —
    // see `board_bench::check`/`take`).
    let holder = args.holder.as_deref();
    let resolved_port = match (args.mac.as_deref(), args.port.as_deref()) {
        (Some(mac), None) => port::resolve_by_mac(mac, holder)?,
        (None, Some(p)) => port::resolve_checked(Some(p), None, holder)?,
        _ => bail!("pass exactly one of --mac or --port"),
    };

    let target = match &args.target {
        Some(target) => target.clone(),
        None => flash::read_target(&resolved_port)
            .context("could not read the board's own target; pass --target")?,
    };

    let lookup = LightplayerLookup::new()?;
    let (version, resolve_note) = resolve_version(&request, &target, &lookup)?;
    if let Some(note) = &resolve_note {
        println!("{note}");
    }
    println!("resolved `{}` to {version} for {target}", args.release);

    let scratch = tempfile::Builder::new()
        .prefix("lp-cli-firmware-install-")
        .tempdir()
        .context("creating a scratch directory for the download")?;
    let fetched = fetch_package(&lookup, &target, &version, scratch.path())?;
    if fetched.version != version {
        bail!(
            "downloaded {target}/{version}/package.json but it reports version {}",
            fetched.version
        );
    }
    println!(
        "downloaded and verified {} image{} for {target} {version}:",
        fetched.manifest.images.len(),
        if fetched.manifest.images.len() == 1 {
            ""
        } else {
            "s"
        }
    );
    for image in &fetched.manifest.images {
        println!(
            "  {} — {} bytes, sha256 {}",
            image.path, image.size_bytes, image.sha256
        );
    }

    if args.dry_run {
        println!(
            "dry run: would install {target} {version} on {resolved_port}. Nothing written, the \
             board was not touched."
        );
        return Ok(());
    }

    let manifest_path = fetched.dir.join("manifest.json");
    let leased = lease_board(&resolved_port, holder, &version)?;
    let result = flash::install_package(
        &resolved_port,
        &manifest_path,
        args.backup_dir.as_deref(),
        args.yes,
    );
    if leased {
        if let Err(e) = board_bench::drop_lease(&resolved_port, holder) {
            eprintln!("warning: could not drop the lease on {resolved_port}: {e}");
        }
    }

    match result? {
        flash::WriteOutcome::Declined => {
            println!("Nothing written.");
            Ok(())
        }
        flash::WriteOutcome::Written { migrated } => {
            if migrated {
                println!("The board's files moved to the package's filesystem layout.");
            }
            flash::wait_for_hello(&resolved_port, &version, &target)
        }
    }
}

/// The concrete version to install, and (for `previous`) a note on where
/// the GitHub releases list came from.
fn resolve_version(
    request: &ReleaseRequest,
    target: &str,
    lookup: &LightplayerLookup,
) -> Result<(String, Option<String>)> {
    match request {
        ReleaseRequest::Version(v) => Ok((v.clone(), None)),
        ReleaseRequest::Latest => Ok((fetch_version_at(lookup, target, "latest")?, None)),
        ReleaseRequest::Previous => {
            let latest = fetch_version_at(lookup, target, "latest")?;
            let catalog = default_catalog()?;
            let note = format!(
                "`previous`: looking at the GitHub releases list via {}",
                catalog.source_name()
            );
            let version = previous_version(catalog.as_ref(), target, &latest)?;
            Ok((version, Some(note)))
        }
    }
}

/// `package.json`'s own `core.version` at this lookup segment — the
/// resolved concrete version, whether `segment` was `latest` or already
/// one. Fetched again (pinned to the concrete version) by [`fetch_package`]
/// so `latest` cannot drift between resolving and downloading.
fn fetch_version_at(lookup: &dyn ReleaseFile, target: &str, segment: &str) -> Result<String> {
    let bytes = lookup
        .fetch(target, segment, "package.json")
        .with_context(|| format!("resolving {target}/{segment}"))?;
    let manifest = DistributionManifest::parse(&bytes)
        .with_context(|| format!("{target}/{segment}/package.json"))?;
    manifest
        .core_str("version")
        .map(str::to_string)
        .context("the package's manifest core has no `version`")
}

/// Lease the board for the write, when `board` is on the desk. Returns
/// whether a lease was taken (so the caller knows whether to drop one).
fn lease_board(port: &str, holder: Option<&str>, version: &str) -> Result<bool> {
    if !board_bench::available() {
        return Ok(false);
    }
    board_bench::take(
        port,
        holder,
        &format!("lp-cli firmware install: {version}"),
        None,
    )?;
    Ok(true)
}
