//! Emit linker script arg for ESP32 memory layout.
//!
//! esp-hal copies linker scripts (including linkall.x) to its OUT_DIR and adds
//! link-search. We must tell the linker to use linkall.x explicitly via -T.

fn main() {
    println!("cargo:rustc-link-arg=-Tlinkall.x");
}
