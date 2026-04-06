//! Instantiate shader wasm via `WebAssembly.Instance` with `env.memory` + `builtins` imports.

use js_sys::{Object, Reflect, Uint8Array, WebAssembly};
use wasm_bindgen::JsValue;

use crate::error::WasmError;
use crate::module::EnvMemorySpec;

use super::BrowserLpvmModule;
use super::engine::host_exports;

pub(crate) struct BrowserInstance {
    pub instance: WebAssembly::Instance,
    pub memory: Option<WebAssembly::Memory>,
}

fn import_desc_string(obj: &JsValue, key: &str) -> Option<String> {
    Reflect::get(obj, &JsValue::from_str(key))
        .ok()
        .and_then(|v| v.as_string())
}

fn imports_array_has_module_name(
    imports: &js_sys::Array,
    module: &str,
    name: Option<&str>,
) -> bool {
    for i in 0..imports.length() {
        let desc = imports.get(i);
        if import_desc_string(&desc, "module").as_deref() != Some(module) {
            continue;
        }
        if let Some(n) = name {
            if import_desc_string(&desc, "name").as_deref() != Some(n) {
                continue;
            }
        }
        return true;
    }
    false
}

fn needs_builtins_link(imports: &js_sys::Array) -> bool {
    imports_array_has_module_name(imports, "builtins", None)
}

fn needs_env_memory(imports: &js_sys::Array) -> bool {
    imports_array_has_module_name(imports, "env", Some("memory"))
}

fn build_builtins_import(
    host_exports: &JsValue,
    imports_desc: &js_sys::Array,
) -> Result<JsValue, WasmError> {
    let builtins = Object::new();
    for i in 0..imports_desc.length() {
        let desc = imports_desc.get(i);
        if import_desc_string(&desc, "module").as_deref() != Some("builtins") {
            continue;
        }
        let name = import_desc_string(&desc, "name")
            .ok_or_else(|| WasmError::runtime("builtin import missing name"))?;
        let func = Reflect::get(host_exports, &JsValue::from_str(&name)).map_err(|_| {
            WasmError::runtime(format!("builtin `{name}` missing from host exports"))
        })?;
        if func.is_undefined() || func.is_null() {
            return Err(WasmError::runtime(format!(
                "builtin `{name}` is undefined in host exports"
            )));
        }
        Reflect::set(&builtins, &JsValue::from_str(&name), &func)
            .map_err(|e| WasmError::runtime(format!("Reflect.set builtins.{name}: {e:?}")))?;
    }
    Ok(builtins.into())
}

fn memory_descriptor(spec: &EnvMemorySpec) -> Result<Object, WasmError> {
    let mem_desc = Object::new();
    Reflect::set(
        &mem_desc,
        &JsValue::from_str("initial"),
        &JsValue::from_f64(spec.initial_pages as f64),
    )
    .map_err(|e| WasmError::runtime(format!("memory initial: {e:?}")))?;
    if let Some(max) = spec.max_pages {
        Reflect::set(
            &mem_desc,
            &JsValue::from_str("maximum"),
            &JsValue::from_f64(max as f64),
        )
        .map_err(|e| WasmError::runtime(format!("memory maximum: {e:?}")))?;
    }
    Ok(mem_desc)
}

pub(crate) fn instantiate_shader(module: &BrowserLpvmModule) -> Result<BrowserInstance, WasmError> {
    let wasm_bytes = &module.wasm_bytes;
    let u8 = Uint8Array::new_with_length(wasm_bytes.len() as u32);
    u8.copy_from(wasm_bytes);

    let mod_js: JsValue = u8.into();
    let wasm_module = WebAssembly::Module::new(&mod_js)
        .map_err(|e| WasmError::runtime(format!("WebAssembly.Module: {e:?}")))?;

    let imports_desc = WebAssembly::Module::imports(&wasm_module);
    let needs_builtins = needs_builtins_link(&imports_desc);
    let needs_memory = needs_env_memory(&imports_desc);

    let import_object = Object::new();

    let memory = if needs_memory {
        let spec = module.env_memory.ok_or_else(|| {
            WasmError::runtime("shader imports env.memory but compiler produced no limits")
        })?;
        let mem_desc = memory_descriptor(&spec)?;
        let memory = WebAssembly::Memory::new(&mem_desc)
            .map_err(|e| WasmError::runtime(format!("WebAssembly.Memory: {e:?}")))?;

        let env = Object::new();
        Reflect::set(&env, &JsValue::from_str("memory"), &memory)
            .map_err(|e| WasmError::runtime(format!("set env.memory: {e:?}")))?;
        Reflect::set(&import_object, &JsValue::from_str("env"), &env)
            .map_err(|e| WasmError::runtime(format!("set env: {e:?}")))?;

        Some(memory)
    } else {
        None
    };

    if needs_builtins {
        let hx = host_exports()?;
        let builtins = build_builtins_import(&hx, &imports_desc)?;
        Reflect::set(&import_object, &JsValue::from_str("builtins"), &builtins)
            .map_err(|e| WasmError::runtime(format!("set builtins: {e:?}")))?;
    }

    let instance = WebAssembly::Instance::new(&wasm_module, &import_object)
        .map_err(|e| WasmError::runtime(format!("WebAssembly.Instance: {e:?}")))?;

    Ok(BrowserInstance { instance, memory })
}
