//! Browser/Web Worker LightPlayer runtime proof.

#![cfg(target_arch = "wasm32")]

use std::cell::RefCell;

use js_sys::{Array, Function, Reflect, Uint8Array};
use lps_frontend::{compile, lower};
use lpvm::{LpvmEngine, LpvmModule};
use lpvm_wasm::rt_browser::{BrowserLpvmEngine, BrowserLpvmInstance, init_host_exports};
use lpvm_wasm::{FloatMode, WasmOptions};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

const PIXEL_BUF_OFFSET: u32 = 1024;

thread_local! {
    static RUNTIMES: RefCell<Vec<BrowserRuntime>> = const { RefCell::new(Vec::new()) };
}

struct BrowserRuntime {
    id: u32,
    label: String,
    engine: BrowserLpvmEngine,
    instance: Option<BrowserLpvmInstance>,
    logs: Vec<String>,
}

#[wasm_bindgen]
pub fn fw_browser_init_exports(exports: JsValue) {
    init_host_exports(exports);
}

#[wasm_bindgen]
pub fn create_runtime(label: &str) -> Result<u32, String> {
    let opts = WasmOptions {
        float_mode: FloatMode::Q32,
        ..Default::default()
    };
    let engine = BrowserLpvmEngine::new(opts).map_err(|error| format!("{error}"))?;

    RUNTIMES.with(|runtimes| {
        let mut runtimes = runtimes.borrow_mut();
        let id = runtimes.len() as u32 + 1;
        runtimes.push(BrowserRuntime {
            id,
            label: label.to_string(),
            engine,
            instance: None,
            logs: vec![format!("runtime {id} created: {label}")],
        });
        Ok(id)
    })
}

#[wasm_bindgen]
pub fn runtime_count() -> u32 {
    RUNTIMES.with(|runtimes| runtimes.borrow().len() as u32)
}

#[wasm_bindgen]
pub fn compile_shader(runtime_id: u32, source: &str) -> Result<(), String> {
    with_runtime_mut(runtime_id, |runtime| {
        let naga = compile(source).map_err(|error| format!("parse: {error}"))?;
        let (ir, meta) = lower(&naga).map_err(|error| format!("lower: {error}"))?;
        let module = runtime
            .engine
            .compile(&ir, &meta)
            .map_err(|error| format!("compile: {error}"))?;
        let instance = module
            .instantiate()
            .map_err(|error| format!("instantiate: {error}"))?;
        runtime.instance = Some(instance);
        runtime.logs.push("shader compiled".to_string());
        Ok(())
    })
}

#[wasm_bindgen]
pub fn render_first_pixel(runtime_id: u32, time_q32: i32) -> Result<String, String> {
    with_runtime_mut(runtime_id, |runtime| {
        let instance = runtime
            .instance
            .as_ref()
            .ok_or_else(|| "no shader loaded".to_string())?;
        let exports = instance.js_exports();
        let func = Reflect::get(exports, &JsValue::from_str("render_frame"))
            .map_err(|error| format!("get render_frame: {error:?}"))?;
        let func: Function = func
            .dyn_into()
            .map_err(|_| "render_frame is not a function".to_string())?;

        let args = Array::new();
        args.push(&JsValue::from_f64(1.0));
        args.push(&JsValue::from_f64(1.0));
        args.push(&JsValue::from_f64(time_q32 as f64));
        args.push(&JsValue::from_f64(PIXEL_BUF_OFFSET as f64));
        func.apply(&JsValue::NULL, &args)
            .map_err(|error| format!("render_frame trap: {error:?}"))?;

        let memory = instance
            .js_memory()
            .ok_or_else(|| "shader has no linear memory export".to_string())?;
        let buffer = memory.buffer();
        let bytes = Uint8Array::new_with_byte_offset_and_length(&buffer, PIXEL_BUF_OFFSET, 4);
        let mut rgba = [0_u8; 4];
        bytes.copy_to(&mut rgba);
        runtime.logs.push(format!("rendered first pixel: {rgba:?}"));

        Ok(format!("{},{},{},{}", rgba[0], rgba[1], rgba[2], rgba[3]))
    })
}

#[wasm_bindgen]
pub fn logs(runtime_id: u32) -> Result<String, String> {
    with_runtime_mut(runtime_id, |runtime| {
        Ok(format!(
            "runtime {} ({})\n{}",
            runtime.id,
            runtime.label,
            runtime.logs.join("\n")
        ))
    })
}

fn with_runtime_mut<T>(
    runtime_id: u32,
    f: impl FnOnce(&mut BrowserRuntime) -> Result<T, String>,
) -> Result<T, String> {
    RUNTIMES.with(|runtimes| {
        let mut runtimes = runtimes.borrow_mut();
        let runtime = runtimes
            .iter_mut()
            .find(|runtime| runtime.id == runtime_id)
            .ok_or_else(|| format!("runtime {runtime_id} not found"))?;
        f(runtime)
    })
}

#[cfg(test)]
mod tests {
    use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

    use super::*;

    wasm_bindgen_test_configure!(run_in_browser);

    const CONSTANT_RED_SHADER: &str = r#"
vec4 render(vec2 fragCoord, vec2 outputSize, float time) {
    return vec4(1.0, 0.0, 0.0, 1.0);
}
"#;

    #[wasm_bindgen_test]
    fn compiles_and_renders_constant_shader() {
        fw_browser_init_exports(wasm_bindgen::exports());

        let before_count = runtime_count();
        let runtime_id = create_runtime("wasm-bindgen-test").expect("create runtime");
        assert_eq!(runtime_count(), before_count + 1);

        compile_shader(runtime_id, CONSTANT_RED_SHADER).expect("compile shader");
        let rgba = render_first_pixel(runtime_id, 0).expect("render first pixel");

        assert_ne!(rgba, "0,0,0,0");
        assert!(
            logs(runtime_id)
                .expect("runtime logs")
                .contains("shader compiled"),
            "logs should record shader compilation"
        );
    }
}
