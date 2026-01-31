//! Build script for lp-glsl-compiler
//!
//! This script sets up the path to lp-glsl-builtins static library.
//! The library must be built manually with:
//!   cargo build --target riscv32imac-unknown-none-elf --package lp-glsl-builtins

#[cfg(feature = "emulator")]
fn main() {
    use std::env;

    let target = "riscv32imac-unknown-none-elf";
    let profile = env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());

    // Find workspace root
    let out_dir = env::var("OUT_DIR").unwrap();
    let workspace_root = find_workspace_root(&out_dir)
        .expect("Could not find workspace root (looking for Cargo.toml with [workspace])");

    // Path to the lp-glsl-builtins-emu-app executable
    // Try release first (since build-builtins.sh builds in release mode), then fall back to profile
    let exe_path_release = workspace_root
        .join("../../target")
        .join(target)
        .join("release")
        .join("lp-glsl-builtins-emu-app");
    let exe_path_profile = workspace_root
        .join("../../target")
        .join(target)
        .join(&profile)
        .join("lp-glsl-builtins-emu-app");

    // Prefer release build, fall back to profile-specific build
    let exe_path = if exe_path_release.exists() {
        exe_path_release
    } else if exe_path_profile.exists() {
        exe_path_profile.clone()
    } else {
        // Neither exists, use release path for error message
        exe_path_release
    };

    // Check if executable exists and copy to OUT_DIR for compile-time inclusion
    if !exe_path.exists() {
        println!(
            "cargo:warning=lp-glsl-builtins-emu-app executable not found at: {}",
            exe_path.display()
        );
        println!("cargo:warning=Also checked: {}", exe_path_profile.display());
        println!("cargo:warning=Build it manually with: scripts/build-builtins.sh");
        // Generate empty bytes if executable doesn't exist
        let out_file = std::path::Path::new(&out_dir).join("lp_builtins_lib.rs");
        std::fs::write(&out_file, "pub const LP_BUILTINS_EXE_BYTES: &[u8] = &[];\n")
            .expect("Failed to write empty builtins exe file");
    } else {
        // Executable found - set up rerun-if-changed (no warning needed for success case)
        println!("cargo:rerun-if-changed={}", exe_path.display());
        // Copy executable to OUT_DIR
        let out_file = std::path::Path::new(&out_dir).join("lp-glsl-builtins-emu-app");
        std::fs::copy(&exe_path, &out_file)
            .expect("Failed to copy lp-glsl-builtins-emu-app executable to OUT_DIR");

        // Generate a module that includes the executable bytes
        let include_file = std::path::Path::new(&out_dir).join("lp_builtins_lib.rs");
        let include_path = out_file
            .strip_prefix(&out_dir)
            .expect("Failed to get relative path")
            .to_string_lossy()
            .replace('\\', "/");
        std::fs::write(
            &include_file,
            format!(
                "pub const LP_BUILTINS_EXE_BYTES: &[u8] = include_bytes!(\"{include_path}\");\n"
            ),
        )
        .expect("Failed to write builtins exe include file");
    }

    // Tell Cargo to rerun if lp-glsl-builtins-emu-app source changes
    let builtins_app_path = workspace_root.join("apps").join("lp-glsl-builtins-emu-app");
    println!(
        "cargo:rerun-if-changed={}",
        builtins_app_path.join("../../Cargo.toml").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        builtins_app_path.join("src").display()
    );
    // Also rerun if the executable changes (in case it's rebuilt externally)
    println!("cargo:rerun-if-changed={}", exe_path.display());
}

#[cfg(not(feature = "emulator"))]
fn main() {
    // No-op when emulator feature is disabled
}

/// Find the workspace root by looking for Cargo.toml with [workspace]
#[allow(dead_code, reason = "Used for workspace detection in build script")]
fn find_workspace_root(start: &str) -> Option<std::path::PathBuf> {
    use std::path::Path;
    let mut current = Path::new(start);

    loop {
        let cargo_toml = current.join("../../Cargo.toml");
        if cargo_toml.exists() {
            if let Ok(contents) = std::fs::read_to_string(&cargo_toml) {
                if contents.contains("[workspace]") {
                    return Some(current.to_path_buf());
                }
            }
        }

        if let Some(parent) = current.parent() {
            current = parent;
        } else {
            break;
        }
    }

    None
}
