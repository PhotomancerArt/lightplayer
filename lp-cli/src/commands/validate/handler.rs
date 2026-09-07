use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use lp_emu_validate::ValidateConfig;
use lp_emu_validate::replay::ReplayOptions;
use lp_emu_validate::run::{self, ImageOverrides, RecordProvenance, RunOptions};

use super::args::{RecordArgs, ReplayArgs, ValidateCli, ValidateCommand};

pub fn handle_validate(cli: ValidateCli) -> Result<()> {
    let repo_root = resolve_repo_root(cli.repo_root.as_deref())?;
    let config = ValidateConfig::embedded();

    match cli.command {
        ValidateCommand::List => {
            print!("{}", run::list(&config, &repo_root)?);
            Ok(())
        }
        ValidateCommand::Replay(args) => replay(args, &repo_root),
        ValidateCommand::Run(args) => {
            let images = ImageOverrides::parse(&args.image)?;
            print!(
                "{}",
                run::run_set(
                    &config,
                    &args.set,
                    &args.configuration,
                    &RunOptions {
                        port: args.port.as_deref(),
                        images: &images,
                        timeout_secs: args.timeout_secs,
                    },
                    &repo_root,
                    args.dry_run,
                )?
            );
            Ok(())
        }
        ValidateCommand::Record(args) => record(args, &config, &repo_root),
    }
}

fn replay(args: ReplayArgs, repo_root: &Path) -> Result<()> {
    let options = ReplayOptions {
        strict: args.strict,
        strict_timing: args.strict_timing,
    };
    // `--against` is a path if it exists on disk, and a configuration name
    // otherwise. Configuration names carry a `:`, which no transcript filename
    // does, so the two cannot be confused.
    let against = Path::new(&args.against);
    let (report, text) = if against.exists() {
        run::replay_files(&args.transcript, against, options)?
    } else {
        run::replay_against_configuration(&args.transcript, &args.against, repo_root, options)?
    };
    print!("{text}");
    if report.is_ok() {
        Ok(())
    } else {
        bail!("replay failed with {} problem(s)", report.failures().len())
    }
}

fn record(args: RecordArgs, config: &ValidateConfig, repo_root: &Path) -> Result<()> {
    let date = match args.date {
        Some(d) => d,
        None => chrono::Local::now().format("%Y-%m-%d").to_string(),
    };
    let images = ImageOverrides::parse(&args.image)?;
    print!(
        "{}",
        run::record_set(
            config,
            &args.set,
            &args.configuration,
            &RunOptions {
                port: args.port.as_deref(),
                images: &images,
                timeout_secs: args.timeout_secs,
            },
            repo_root,
            &RecordProvenance {
                date: &date,
                firmware_commit: &args.firmware_commit,
                firmware_dirty: Some(args.firmware_dirty),
            },
            args.dry_run,
        )?
    );
    Ok(())
}

/// The workspace root: an explicit `--repo-root`, or the nearest ancestor of
/// the current directory whose `Cargo.toml` declares `[workspace]`.
fn resolve_repo_root(explicit: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = explicit {
        if !path.is_dir() {
            bail!("--repo-root {} is not a directory", path.display());
        }
        return Ok(path.to_path_buf());
    }
    let cwd = std::env::current_dir().context("resolving the current directory")?;
    for dir in cwd.ancestors() {
        let manifest = dir.join("Cargo.toml");
        if manifest.is_file()
            && std::fs::read_to_string(&manifest).is_ok_and(|text| text.contains("[workspace]"))
        {
            return Ok(dir.to_path_buf());
        }
    }
    bail!(
        "no workspace root above {} — pass --repo-root",
        cwd.display()
    )
}
