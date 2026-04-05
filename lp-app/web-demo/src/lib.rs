//! GLSL → WASM compiler exposed to JavaScript via wasm-bindgen.

use lps_wasm::{glsl_wasm, WasmOptions};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn compile_glsl(source: &str) -> Result<Vec<u8>, String> {
    let options = WasmOptions::default();
    match glsl_wasm(source, options) {
        Ok(module) => Ok(module.bytes),
        Err(e) => Err(e.to_string()),
    }
}
