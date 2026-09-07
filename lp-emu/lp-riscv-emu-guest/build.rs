fn main() {
    println!("cargo:rerun-if-changed=memory.ld");
    println!(
        "cargo:rustc-link-search=native={}",
        std::env::var("CARGO_MANIFEST_DIR").unwrap()
    );
    println!("cargo:rustc-link-arg=-Tmemory.ld");
}
