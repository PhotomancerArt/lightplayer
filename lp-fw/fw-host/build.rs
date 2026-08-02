//! Emits the host target triple and cargo profile for the embedded firmware
//! manifest — fw-host is the one embedder whose target varies per machine.

fn main() {
    println!(
        "cargo:rustc-env=LP_CARGO_TARGET={}",
        std::env::var("TARGET").expect("TARGET")
    );
    println!(
        "cargo:rustc-env=LP_BUILD_PROFILE={}",
        std::env::var("PROFILE").expect("PROFILE")
    );
}
