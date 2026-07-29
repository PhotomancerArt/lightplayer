//! Link with this crate's own `link.ld` (a code-heavy split of the same
//! region the fixtures script uses — see its header) and no host startup files.

fn main() {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let script = std::path::Path::new(&manifest).join("link.ld");
    println!("cargo:rustc-link-arg=-nostartfiles");
    println!("cargo:rustc-link-arg=-T{}", script.display());
    println!("cargo:rerun-if-changed={}", script.display());
}
