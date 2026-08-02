//! `lp-cli firmware build <id>` — cargo-build one firmware variant from its
//! build def.

use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};

use super::args::BuildArgs;
use super::build_def::{BuildDef, find_repo_root, load_build_def};

pub fn handle_build(args: BuildArgs) -> Result<()> {
    let repo_root = find_repo_root()?;
    let def = load_build_def(&repo_root, &args.id)?;
    build_firmware(&repo_root, &def)?;
    println!("built {}", def.elf_path(&repo_root).display());
    Ok(())
}

/// Run the def's cargo build. Executed **in the crate directory** so the
/// crate-local `.cargo/config.toml` (linker scripts, build-std) and
/// `rust-toolchain.toml` (the Xtensa fork's `esp` channel) apply — building
/// from the workspace root fails at link time instead.
pub fn build_firmware(repo_root: &Path, def: &BuildDef) -> Result<()> {
    let crate_dir = def.crate_dir(repo_root)?;
    let features = def.cargo_features.join(",");
    let mut command = Command::new("cargo");
    command
        .current_dir(&crate_dir)
        .arg("build")
        .args(["--target", &def.cargo_target])
        .args(["--profile", &def.profile])
        .args(["--features", &features]);
    scrub_outer_build_env(&mut command);

    println!(
        "building {} ({} / {} / {})",
        def.id, def.package, def.cargo_target, def.profile
    );
    let status = command
        .status()
        .with_context(|| format!("running cargo build in {}", crate_dir.display()))?;
    if !status.success() {
        bail!("cargo build failed for firmware build `{}`", def.id);
    }

    let elf = def.elf_path(repo_root);
    if !elf.exists() {
        bail!(
            "cargo build for `{}` succeeded but {} does not exist",
            def.id,
            elf.display()
        );
    }
    Ok(())
}

/// Drop the outer build's toolchain environment from the child cargo.
///
/// `lp-cli` is normally launched by `cargo run`, and rustup exports
/// `RUSTUP_TOOLCHAIN` (plus `RUSTC`/`CARGO` paths) into it. Those **override**
/// a directory's `rust-toolchain.toml`, so a nested build silently ran on the
/// host's pinned nightly instead of Espressif's fork — the S3 then failed with
/// `data-layout ... differs from LLVM target's xtensa-none-elf default`, which
/// reads like a target-spec bug and is not one. Removing them lets rustup
/// resolve the toolchain from the crate directory, which is the whole reason
/// the build runs there. `RUSTFLAGS` goes too: the host's flags have no
/// business in a bare-metal image.
fn scrub_outer_build_env(command: &mut Command) {
    for key in [
        "RUSTUP_TOOLCHAIN",
        "RUSTC",
        "RUSTDOC",
        "CARGO",
        "CARGO_MAKEFLAGS",
        "RUSTFLAGS",
        "CARGO_ENCODED_RUSTFLAGS",
        "CARGO_MANIFEST_DIR",
        "CARGO_MANIFEST_PATH",
    ] {
        command.env_remove(key);
    }
}
