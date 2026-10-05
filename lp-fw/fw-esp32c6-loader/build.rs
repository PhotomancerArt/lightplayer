//! Rebuild when the linker script changes, and let the linker find it from
//! wherever cargo runs it.
fn main() {
    let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    println!("cargo:rustc-link-search={dir}");
    println!("cargo:rerun-if-changed=loader.x");
}
