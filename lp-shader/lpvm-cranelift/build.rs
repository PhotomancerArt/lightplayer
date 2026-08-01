//! When `riscv32-object` is enabled, embeds `lps-builtins-emu-app` for linking tests.
//! Build the executable with `scripts/build-builtins.sh` from the workspace root.

fn main() {
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR");
    let out_path = std::path::Path::new(&out_dir).join("lp_builtins_lib.rs");

    if std::env::var("CARGO_FEATURE_RISCV32_OBJECT").is_err() {
        std::fs::write(&out_path, "pub const LP_BUILTINS_EXE_BYTES: &[u8] = &[];\n")
            .expect("write lp_builtins_lib.rs");
        return;
    }

    // Walk up from the crate's manifest dir, NOT from OUT_DIR: with a
    // configured `build.build-dir`, OUT_DIR lives outside the workspace
    // entirely, while the manifest dir is always inside it.
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let workspace_root = find_workspace_root(&manifest_dir).expect("workspace root");
    let target = "riscv32imac-unknown-none-elf";
    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());

    let exe_path_release = workspace_root
        .join("target")
        .join(target)
        .join("release")
        .join("lps-builtins-emu-app");
    let exe_path_profile = workspace_root
        .join("target")
        .join(target)
        .join(&profile)
        .join("lps-builtins-emu-app");
    println!("cargo:rerun-if-changed={}", exe_path_release.display());
    println!("cargo:rerun-if-changed={}", exe_path_profile.display());

    let exe_path = if exe_path_release.exists() {
        exe_path_release
    } else if exe_path_profile.exists() {
        exe_path_profile.clone()
    } else {
        exe_path_release
    };

    let copied = std::path::Path::new(&out_dir).join("lps-builtins-emu-app");
    if let Err(reason) = copy_builtins_exe(&exe_path, &copied) {
        println!(
            "cargo:warning=lps-builtins-emu-app unusable at {} ({reason}) — run scripts/build-builtins.sh",
            exe_path.display()
        );
        std::fs::write(&out_path, "pub const LP_BUILTINS_EXE_BYTES: &[u8] = &[];\n")
            .expect("write empty lp_builtins_lib.rs");
        return;
    }
    let rel = copied
        .strip_prefix(&out_dir)
        .expect("relative to OUT_DIR")
        .to_string_lossy()
        .replace('\\', "/");
    std::fs::write(
        &out_path,
        format!("pub const LP_BUILTINS_EXE_BYTES: &[u8] = include_bytes!(\"{rel}\");\n"),
    )
    .expect("write lp_builtins_lib.rs");
}

/// Copy the builtins ELF into `OUT_DIR`, retrying past a concurrent rewrite.
///
/// `scripts/build-builtins.sh` relinks this exe, and cargo uplifts the result
/// by remove-then-hardlink: there is a window in which the path does not
/// exist (or, on a cross-device copy, is short). Cargo's build lock is
/// profile+triple scoped — `target/debug/.cargo-lock` for a host build vs
/// `target/riscv32imac-unknown-none-elf/release/.cargo-lock` for that script
/// — so a host build is *not* excluded from the window. Landing in it is not
/// a remote possibility either: this build script declares
/// `rerun-if-changed` on the very path the script rewrites, so the rewrite is
/// exactly what wakes us. Treating the window as "not built" embedded an
/// empty slice and surfaced minutes later as every `NativeEmuEngine` test
/// failing instantly with "builtins ... not found at build time".
/// See docs/defects/2026-07-29-builtins-elf-uplift-race.md.
fn copy_builtins_exe(src: &std::path::Path, dst: &std::path::Path) -> Result<(), String> {
    let deadline =
        std::time::Instant::now() + std::time::Duration::from_secs(BUILTINS_RETRY_BUDGET_SECS);
    loop {
        let reason = match try_copy_builtins_exe(src, dst) {
            Ok(()) => return Ok(()),
            Err(reason) => reason,
        };
        // Only a workspace that has already produced rv32 release artifacts can
        // have a build racing us; on a fresh clone the directory is absent and
        // there is nothing to wait for, so report "missing" immediately.
        let rv32_dir_exists = src.parent().is_some_and(|d| d.is_dir());
        if !rv32_dir_exists || std::time::Instant::now() >= deadline {
            return Err(reason);
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

const BUILTINS_RETRY_BUDGET_SECS: u64 = 2;

/// One attempt: read, verify it is a whole ELF image, then write it out.
fn try_copy_builtins_exe(src: &std::path::Path, dst: &std::path::Path) -> Result<(), String> {
    let bytes = std::fs::read(src).map_err(|e| format!("read: {e}"))?;
    if !bytes.starts_with(b"\x7fELF") {
        return Err(format!("not an ELF image ({} bytes)", bytes.len()));
    }
    // A copy still in flight would read short but still carry valid magic;
    // a size that moved across the read means we caught a partial write.
    let size_after = std::fs::metadata(src)
        .map_err(|e| format!("stat: {e}"))?
        .len();
    if size_after != bytes.len() as u64 {
        return Err(format!(
            "changed size during read ({} → {size_after} bytes)",
            bytes.len()
        ));
    }
    std::fs::write(dst, &bytes).map_err(|e| format!("write to OUT_DIR: {e}"))
}

fn find_workspace_root(start: &str) -> Option<std::path::PathBuf> {
    let mut dir = std::path::Path::new(start);
    loop {
        let cargo_toml = dir.join("Cargo.toml");
        if cargo_toml.exists() {
            if let Ok(contents) = std::fs::read_to_string(&cargo_toml) {
                if contents.contains("[workspace]") {
                    return Some(dir.to_path_buf());
                }
            }
        }
        dir = dir.parent()?;
    }
}
