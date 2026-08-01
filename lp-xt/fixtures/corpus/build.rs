//! Link every fixture bin with the fixtures/link.ld script (addresses match
//! lp-xt-emu's modeled SRAM1) and without host startup files.

fn main() {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let script = std::path::Path::new(&manifest).join("../link.ld");
    println!("cargo:rustc-link-arg=-nostartfiles");
    println!("cargo:rustc-link-arg=-T{}", script.display());
    println!("cargo:rerun-if-changed={}", script.display());
}
