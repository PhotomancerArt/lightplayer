//! Put `loader.x` on the linker's search path and use it.
fn main() {
    let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    println!("cargo:rustc-link-search={dir}");
    println!("cargo:rustc-link-arg-bins=-Wl,-Tloader.x");
    println!("cargo:rerun-if-changed=loader.x");
}
