//! When `riscv32-emu` is enabled, embeds `lps-builtins-emu-app` for linking tests.
//! Build the executable with `scripts/build-builtins.sh` from the workspace root.

fn main() {
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR");
    let out_path = std::path::Path::new(&out_dir).join("lp_builtins_lib.rs");

    if std::env::var("CARGO_FEATURE_RISCV32_EMU").is_err() {
        std::fs::write(&out_path, "pub const LP_BUILTINS_EXE_BYTES: &[u8] = &[];\n")
            .expect("write lp_builtins_lib.rs");
        return;
    }

    let workspace_root = find_workspace_root(&out_dir).expect("workspace root");
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

    let exe_path = if exe_path_release.exists() {
        exe_path_release
    } else if exe_path_profile.exists() {
        exe_path_profile.clone()
    } else {
        exe_path_release
    };

    if !exe_path.exists() {
        println!(
            "cargo:warning=lps-builtins-emu-app not found at {} — run scripts/build-builtins.sh",
            exe_path.display()
        );
        std::fs::write(&out_path, "pub const LP_BUILTINS_EXE_BYTES: &[u8] = &[];\n")
            .expect("write empty lp_builtins_lib.rs");
        return;
    }

    println!("cargo:rerun-if-changed={}", exe_path.display());
    let copied = std::path::Path::new(&out_dir).join("lps-builtins-emu-app");
    std::fs::copy(&exe_path, &copied).expect("copy builtins exe");
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
