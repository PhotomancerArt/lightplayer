fn main() {
    // Use the linker script from lp-riscv-emu-guest crate
    // CARGO_MANIFEST_DIR points to fw-emu directory
    // We need to go up to fw-emu/, then to lp-fw/, then to root/, then to lp-glsl/, then to crates/, then to lp-riscv-emu-guest
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");

    // Go from lp-fw/fw-emu to lp-glsl/crates/lp-riscv-emu-guest
    let emu_guest_path = std::path::Path::new(&manifest_dir)
        .parent() // lp-fw/
        .and_then(|p| p.parent()) // root/
        .and_then(|p| {
            p.join("lp-glsl")
                .join("crates")
                .join("lp-riscv-emu-guest")
                .canonicalize()
                .ok()
        })
        .expect("Failed to find lp-riscv-emu-guest crate directory");

    let linker_script = emu_guest_path.join("memory.ld");

    if !linker_script.exists() {
        panic!(
            "Linker script not found at: {}. Ensure lp-riscv-emu-guest crate exists.",
            linker_script.display()
        );
    }

    println!("cargo:rerun-if-changed={}", linker_script.display());
    println!(
        "cargo:rustc-link-search=native={}",
        emu_guest_path.display()
    );
    println!("cargo:rustc-link-arg=-Tmemory.ld");
}
