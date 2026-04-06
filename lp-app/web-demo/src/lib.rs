//! GLSL → LPIR → WASM in the browser; builtins come from this same module’s exports (`lpvm_init_exports`).

use std::cell::RefCell;

use js_sys::{Array, Function, Reflect};
use lps_frontend::{compile, lower};
use lpvm::{LpvmEngine, LpvmModule};
use lpvm_wasm::rt_browser::{BrowserLpvmEngine, BrowserLpvmInstance, init_host_exports};
use lpvm_wasm::{FloatMode, WasmOptions};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

thread_local! {
    static ENGINE: RefCell<Option<BrowserLpvmEngine>> = RefCell::new(None);
    static INSTANCE: RefCell<Option<BrowserLpvmInstance>> = RefCell::new(None);
}

/// Called once after wasm-bindgen `init()`, passing `instance.exports` from this same wasm module.
#[wasm_bindgen]
pub fn lpvm_init_exports(exports: JsValue) {
    init_host_exports(exports);
}

#[wasm_bindgen]
pub fn init_engine() {
    let opts = WasmOptions {
        float_mode: FloatMode::Q32,
    };
    ENGINE.with(|e| *e.borrow_mut() = Some(BrowserLpvmEngine::new(opts)));
}

#[wasm_bindgen]
pub fn compile_shader(source: &str) -> Result<(), String> {
    let naga = compile(source).map_err(|e| format!("parse: {e}"))?;
    let (ir, meta) = lower(&naga).map_err(|e| format!("lower: {e}"))?;

    ENGINE.with(|eng| {
        let eng = eng.borrow();
        let engine = eng
            .as_ref()
            .ok_or_else(|| "init_engine not called".to_string())?;
        let module = engine
            .compile(&ir, &meta)
            .map_err(|e| format!("compile: {e}"))?;
        let instance = module
            .instantiate()
            .map_err(|e| format!("instantiate: {e}"))?;
        INSTANCE.with(|i| *i.borrow_mut() = Some(instance));
        Ok(())
    })
}

#[wasm_bindgen]
pub fn shader_ready() -> bool {
    INSTANCE.with(|i| i.borrow().is_some())
}

#[wasm_bindgen]
pub fn render_frame(width: i32, height: i32, time_q32: i32, out_ptr: i32) -> Result<(), String> {
    INSTANCE.with(|i| {
        let i = i.borrow();
        let instance = i.as_ref().ok_or_else(|| "no shader loaded".to_string())?;

        let exports = instance.js_exports();
        let func = Reflect::get(exports, &JsValue::from_str("render_frame"))
            .map_err(|e| format!("get render_frame: {e:?}"))?;
        let func: Function = func
            .dyn_into()
            .map_err(|_| "render_frame is not a function".to_string())?;

        let args = Array::new();
        args.push(&JsValue::from_f64(width as f64));
        args.push(&JsValue::from_f64(height as f64));
        args.push(&JsValue::from_f64(time_q32 as f64));
        args.push(&JsValue::from_f64(out_ptr as f64));

        func.apply(&JsValue::NULL, &args)
            .map_err(|e| format!("render_frame trap: {e:?}"))?;
        Ok(())
    })
}

#[wasm_bindgen]
pub fn get_shader_memory() -> JsValue {
    INSTANCE.with(|i| {
        let i = i.borrow();
        match i.as_ref().and_then(|inst| inst.js_memory()) {
            Some(mem) => mem.clone().into(),
            None => JsValue::NULL,
        }
    })
}
