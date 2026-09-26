//! Embed `lp-xt/fixtures/elf/lps-builtins-xt-app.elf` at build time.
//!
//! Build the image with `scripts/build-builtins-xt.sh` (needs the esp
//! toolchain). When it is absent this embeds an empty slice — a first-class
//! state, not an error: consumers skip the Xtensa host path rather than fail, so
//! the workspace builds and tests on a machine with no esp toolchain.

fn main() {
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR");
    let out_path = std::path::Path::new(&out_dir).join("xt_builtins_image.rs");

    // Walk up from the manifest dir, NOT OUT_DIR: with a configured
    // `build.build-dir`, OUT_DIR lives outside the workspace entirely.
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let workspace_root = find_workspace_root(&manifest_dir).expect("workspace root");
    let elf_path = workspace_root
        .join("lp-xt")
        .join("fixtures")
        .join("elf")
        .join("lps-builtins-xt-app.elf");
    // Sampled before `watch_elf` may create the directory: the retry in
    // `copy_image` must still see a fresh clone as fresh.
    let elf_dir_existed = elf_path.parent().is_some_and(|d| d.is_dir());
    watch_elf(&elf_path);

    let copied = std::path::Path::new(&out_dir).join("lps-builtins-xt-app.elf");
    if let Err(reason) = copy_image(&elf_path, &copied, elf_dir_existed) {
        println!(
            "cargo:warning=lps-builtins-xt-app.elf unusable at {} ({reason}) — run \
             scripts/build-builtins-xt.sh; the Xtensa host-emulation path will be unavailable",
            elf_path.display()
        );
        std::fs::write(&out_path, "pub const IMAGE_BYTES: &[u8] = &[];\n")
            .expect("write empty xt_builtins_image.rs");
        return;
    }
    let rel = copied
        .strip_prefix(&out_dir)
        .expect("relative to OUT_DIR")
        .to_string_lossy()
        .replace('\\', "/");
    std::fs::write(
        &out_path,
        format!("pub const IMAGE_BYTES: &[u8] = include_bytes!(\"{rel}\");\n"),
    )
    .expect("write xt_builtins_image.rs");
}

/// Copy the image into `OUT_DIR`, retrying past a concurrent rewrite.
///
/// Same hazard as the rv32 builtins ELF, and the same fix:
/// `scripts/build-builtins-xt.sh` rewrites this path while other builds may be
/// reading it, and this build script declares `rerun-if-changed` on exactly the
/// path being rewritten — so the rewrite is what wakes us. Treating that window
/// as "not built" would embed an empty slice and surface minutes later as every
/// Xtensa test skipping or failing at once. See
/// `docs/defects/2026-07-29-builtins-elf-uplift-race.md`.
fn copy_image(
    src: &std::path::Path,
    dst: &std::path::Path,
    elf_dir_existed: bool,
) -> Result<(), String> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(RETRY_BUDGET_SECS);
    loop {
        let reason = match try_copy_image(src, dst) {
            Ok(()) => return Ok(()),
            Err(reason) => reason,
        };
        // Only a workspace that already had an elf/ directory can have a build
        // racing us; without one there is nothing to wait for, so report
        // "missing" immediately rather than stalling every fresh clone.
        if !elf_dir_existed || std::time::Instant::now() >= deadline {
            return Err(reason);
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

const RETRY_BUDGET_SECS: u64 = 2;

/// Declare `rerun-if-changed` for the builtins ELF path.
///
/// Cargo treats a `rerun-if-changed` path that does not exist as *always*
/// stale, so watching the ELF path itself while it is absent reruns this
/// script — and recompiles everything above lps-builtins-xt-image — on every
/// cargo invocation in a checkout that has not built the image. When the ELF
/// is absent, watch its directory instead (creating it, since cargo also
/// treats a missing directory as stale). Cargo scans a watched directory
/// recursively, so the ELF appearing — or being relinked by the
/// remove-then-hardlink uplift, which changes the directory — reruns us.
/// Once the ELF exists, the next run watches the file itself again.
///
/// Twin of `watch_builtins_exe` in `lp-shader/lpvm-cranelift/build.rs`
/// (build scripts cannot share code without a new dependency).
fn watch_elf(exe: &std::path::Path) {
    if exe.exists() {
        println!("cargo:rerun-if-changed={}", exe.display());
        return;
    }
    let dir = exe.parent().expect("exe path has a parent");
    if !dir.is_dir() {
        if let Err(e) = std::fs::create_dir_all(dir) {
            // Cannot make a stable watch; fall back to the always-stale one
            // so a later build of the ELF is never missed.
            println!("cargo:warning=could not create {} ({e})", dir.display());
            println!("cargo:rerun-if-changed={}", exe.display());
            return;
        }
        // A directory created during this run is newer than cargo's record
        // of it, which would cost one more rerun on the next build. Backdate
        // it; anything written into it later still moves its mtime. This runs
        // before the copy attempt, so an ELF landing in between is either
        // read by the copy or seen by the watch. Best effort: on failure the
        // cost is that one extra rerun.
        let _ = std::fs::File::open(dir).and_then(|f| f.set_modified(std::time::UNIX_EPOCH));
    }
    println!("cargo:rerun-if-changed={}", dir.display());
}

/// One attempt: read, verify it is a whole ELF image, then write it out.
fn try_copy_image(src: &std::path::Path, dst: &std::path::Path) -> Result<(), String> {
    let bytes = std::fs::read(src).map_err(|e| format!("read: {e}"))?;
    if !bytes.starts_with(b"\x7fELF") {
        return Err(format!("not an ELF image ({} bytes)", bytes.len()));
    }
    // A copy still in flight reads short but keeps valid magic; a size that
    // moved across the read means we caught a partial write.
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
