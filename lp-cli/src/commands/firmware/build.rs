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
    if def.split {
        println!("built {}", def.split_dir(&repo_root).display());
    } else {
        println!("built {}", def.elf_path(&repo_root).display());
    }
    Ok(())
}

/// Run the def's cargo build. Executed **in the crate directory** so the
/// crate-local `.cargo/config.toml` (linker scripts, build-std) and
/// `rust-toolchain.toml` (the Xtensa fork's `esp` channel) apply — building
/// from the workspace root fails at link time instead.
///
/// Either way the build is told its **target** (`LP_FW_TARGET`, the def's
/// id), which the image embeds in its manifest core. A split def builds the
/// split image through `lp_fw_split` into [`BuildDef::split_dir`].
pub fn build_firmware(repo_root: &Path, def: &BuildDef) -> Result<()> {
    if def.split {
        return build_split(repo_root, def);
    }
    let crate_dir = def.crate_dir(repo_root)?;
    let features = def.cargo_features.join(",");
    let mut command = Command::new("cargo");
    command
        .current_dir(&crate_dir)
        .arg("build")
        .args(["--target", &def.cargo_target])
        .args(["--profile", &def.profile])
        .args(["--features", &features])
        .env("LP_FW_TARGET", &def.id);
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

/// The split image, through the split tool's library: two passes, the
/// verifier, the loader, `app.bin` and `merged.bin`.
fn build_split(repo_root: &Path, def: &BuildDef) -> Result<()> {
    if def.cargo_target != lp_fw_split::pass_link::TARGET
        || def.profile != lp_fw_split::pass_link::PROFILE
    {
        bail!(
            "build def `{}` asks for {} / {}, but the split image is built for {} / {}",
            def.id,
            def.cargo_target,
            def.profile,
            lp_fw_split::pass_link::TARGET,
            lp_fw_split::pass_link::PROFILE
        );
    }
    println!(
        "building {} as a split image ({} / {})",
        def.id, def.cargo_target, def.profile
    );
    let mut opts =
        lp_fw_split::BuildOptions::new(repo_root.to_path_buf(), def.split_dir(repo_root));
    opts.features = def.cargo_features.join(",");
    opts.bootloader = def.bootloader_path(repo_root)?;
    opts.partitions = repo_root.join(&def.partitions_csv);
    opts.target = Some(def.id.clone());
    lp_fw_split::build(&opts)
        .with_context(|| format!("building the split image for `{}`", def.id))?;
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
