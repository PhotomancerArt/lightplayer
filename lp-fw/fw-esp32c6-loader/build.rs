//! Pick the linker script for the flavour being built (`loader.x` for the
//! factory loader, `bootloader.x` for the second stage at flash 0x0), and
//! let the linker find it from wherever cargo runs it.
fn main() {
    let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let script = if std::env::var_os("CARGO_FEATURE_BOOTLOADER").is_some() {
        "bootloader.x"
    } else {
        "loader.x"
    };
    println!("cargo:rustc-link-search={dir}");
    println!("cargo:rustc-link-arg-bins=-T{script}");
    println!("cargo:rerun-if-changed=loader.x");
    println!("cargo:rerun-if-changed=bootloader.x");
}
