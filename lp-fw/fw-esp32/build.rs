fn main() {
    // Force 8-byte alignment for .rodata section to prevent bootloader from
    // splitting .rodata_desc and .rodata into separate MAP segments.
    // This is required because the ESP32 bootloader expects at most 2 MAP segments
    // (DROM/IROM), but with 4-byte alignment, the conversion tool creates 3 segments.
    //
    // We use a combination of:
    // 1. An 8-byte aligned constant in main.rs to force .rodata alignment
    // 2. A linker script fragment to ensure .rodata_desc is also 8-byte aligned
    //    and placed contiguously with .rodata
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
    println!("cargo:rerun-if-changed=rodata-align.x");
    println!("cargo:rustc-link-search=native={}", manifest_dir);
    // Include our linker script fragment AFTER esp-rs's default linker script
    // This allows us to override section definitions without redefining memory regions
    println!("cargo:rustc-link-arg=-Trodata-align.x");
}
