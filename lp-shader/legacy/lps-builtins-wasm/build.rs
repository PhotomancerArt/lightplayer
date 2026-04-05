//! Ensure `env.memory` is imported (shared with the shader module).

fn main() {
    let target = std::env::var("TARGET").unwrap_or_default();
    if target == "wasm32-unknown-unknown" {
        println!("cargo:rustc-link-arg=--import-memory");
    }
}
