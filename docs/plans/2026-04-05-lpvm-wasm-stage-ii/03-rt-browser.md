# Phase 3: Implement `rt_browser`

## Scope

Create the browser runtime backend (`rt_browser/`) implementing `LpvmEngine`,
`LpvmModule`, `LpvmInstance` using `js_sys::WebAssembly` APIs. This backend
is compiled only for `target_arch = "wasm32"`.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment.

## Implementation

### 1. Create `rt_browser/mod.rs`

```rust
//! Browser WebAssembly runtime: compile + instantiate + call via js_sys.

mod engine;
mod instance;
mod link;
mod marshal;

pub use engine::{BrowserLpvmEngine, BrowserLpvmModule};
pub use instance::BrowserLpvmInstance;
```

### 2. Create `rt_browser/engine.rs`

`BrowserLpvmEngine` holds compile options and a reference to the host
exports (set via `init_exports()`).

```rust
use js_sys::JsString;
use wasm_bindgen::JsValue;
use lpir::IrModule;
use lps_shared::LpsModuleSig;
use lpvm::LpvmEngine;
use std::collections::HashMap;
use std::cell::RefCell;

use crate::compile::compile_lpir;
use crate::error::WasmError;
use crate::module::WasmExport;
use crate::options::WasmOptions;

use super::instance::BrowserLpvmInstance;

thread_local! {
    static HOST_EXPORTS: RefCell<Option<JsValue>> = RefCell::new(None);
}

/// Call once after wasm-bindgen init, passing `instance.exports`.
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn lpvm_init_exports(exports: JsValue) {
    HOST_EXPORTS.with(|e| *e.borrow_mut() = Some(exports));
    lps_builtins::ensure_builtins_referenced();
}

pub(crate) fn host_exports() -> Result<JsValue, WasmError> {
    HOST_EXPORTS.with(|e| {
        e.borrow()
            .clone()
            .ok_or_else(|| WasmError::runtime("lpvm_init_exports not called"))
    })
}

pub struct BrowserLpvmEngine {
    compile_options: WasmOptions,
}

impl BrowserLpvmEngine {
    pub fn new(compile_options: WasmOptions) -> Self {
        Self { compile_options }
    }
}

pub struct BrowserLpvmModule {
    pub(crate) wasm_bytes: Vec<u8>,
    pub(crate) signatures: LpsModuleSig,
    pub(crate) exports: HashMap<String, WasmExport>,
    pub(crate) shadow_stack_base: Option<i32>,
    pub(crate) opts: WasmOptions,
}

impl LpvmEngine for BrowserLpvmEngine {
    type Module = BrowserLpvmModule;
    type Error = WasmError;

    fn compile(&self, ir: &IrModule, meta: &LpsModuleSig) -> Result<Self::Module, Self::Error> {
        let artifact = compile_lpir(ir, meta, &self.compile_options)?;
        let exports: HashMap<_, _> = artifact
            .wasm_module()
            .exports
            .iter()
            .map(|e| (e.name.clone(), e.clone()))
            .collect();
        Ok(BrowserLpvmModule {
            wasm_bytes: artifact.wasm_module().bytes.clone(),
            signatures: artifact.signatures().clone(),
            exports,
            shadow_stack_base: artifact.wasm_module().shadow_stack_base,
            opts: self.compile_options,
        })
    }
}

impl lpvm::LpvmModule for BrowserLpvmModule {
    type Instance = BrowserLpvmInstance;
    type Error = WasmError;

    fn signatures(&self) -> &LpsModuleSig {
        &self.signatures
    }

    fn instantiate(&self) -> Result<Self::Instance, Self::Error> {
        BrowserLpvmInstance::new(self)
    }
}
```

### 3. Create `rt_browser/link.rs`

Instantiate the shader module using `js_sys::WebAssembly` APIs. Provide
builtins from host exports and create shared memory.

```rust
use js_sys::{Object, Reflect, Uint8Array, WebAssembly};
use wasm_bindgen::JsValue;
use crate::error::WasmError;
use super::engine::host_exports;

pub(crate) struct BrowserInstance {
    pub instance: WebAssembly::Instance,
    pub memory: Option<WebAssembly::Memory>,
}

pub(crate) fn instantiate_shader(
    wasm_bytes: &[u8],
) -> Result<BrowserInstance, WasmError> {
    let buffer = Uint8Array::from(wasm_bytes);
    let module = WebAssembly::Module::new(&buffer.into())
        .map_err(|e| WasmError::runtime(format!("WebAssembly.Module: {e:?}")))?;

    // Inspect module imports to determine what's needed
    let imports_desc = WebAssembly::Module::imports(&module);
    let needs_builtins = needs_builtins_link(&imports_desc);
    let needs_memory = needs_env_memory(&imports_desc);

    let import_object = Object::new();

    let memory = if needs_memory {
        let mem_desc = Object::new();
        Reflect::set(&mem_desc, &"initial".into(), &17.into())
            .map_err(|e| WasmError::runtime(format!("set initial: {e:?}")))?;
        Reflect::set(&mem_desc, &"maximum".into(), &256.into())
            .map_err(|e| WasmError::runtime(format!("set maximum: {e:?}")))?;
        let memory = WebAssembly::Memory::new(&mem_desc)
            .map_err(|e| WasmError::runtime(format!("WebAssembly.Memory: {e:?}")))?;

        let env = Object::new();
        Reflect::set(&env, &"memory".into(), &memory)
            .map_err(|e| WasmError::runtime(format!("set env.memory: {e:?}")))?;
        Reflect::set(&import_object, &"env".into(), &env)
            .map_err(|e| WasmError::runtime(format!("set env: {e:?}")))?;

        Some(memory)
    } else {
        None
    };

    if needs_builtins {
        let exports = host_exports()?;
        let builtins = build_builtins_import(&exports, &imports_desc)?;
        Reflect::set(&import_object, &"builtins".into(), &builtins)
            .map_err(|e| WasmError::runtime(format!("set builtins: {e:?}")))?;
    }

    let instance = WebAssembly::Instance::new(&module, &import_object)
        .map_err(|e| WasmError::runtime(format!("WebAssembly.Instance: {e:?}")))?;

    Ok(BrowserInstance { instance, memory })
}
```

`build_builtins_import` iterates the shader's `"builtins"` imports and
copies the corresponding `WebAssembly.Function` from host exports:

```rust
fn build_builtins_import(
    host_exports: &JsValue,
    imports_desc: &js_sys::Array,
) -> Result<JsValue, WasmError> {
    let builtins = Object::new();
    for i in 0..imports_desc.length() {
        let desc = imports_desc.get(i);
        let module: String = Reflect::get(&desc, &"module".into())?.as_string().unwrap_or_default();
        if module != "builtins" { continue; }
        let name: String = Reflect::get(&desc, &"name".into())?.as_string().unwrap_or_default();
        let func = Reflect::get(host_exports, &name.as_str().into())
            .map_err(|_| WasmError::runtime(format!("builtin {name} not found in host exports")))?;
        if func.is_undefined() {
            return Err(WasmError::runtime(format!("builtin {name} not in host exports")));
        }
        Reflect::set(&builtins, &name.as_str().into(), &func)
            .map_err(|e| WasmError::runtime(format!("set builtins.{name}: {e:?}")))?;
    }
    Ok(builtins.into())
}
```

Helper functions `needs_builtins_link` and `needs_env_memory` inspect the
`WebAssembly.Module.imports()` array for `"builtins"` and `"env"/"memory"`
entries respectively.

### 4. Create `rt_browser/instance.rs`

```rust
use std::collections::HashMap;
use js_sys::{Array, Function, Reflect, WebAssembly};
use wasm_bindgen::JsValue;
use lpir::FloatMode;
use lps_shared::{LpsModuleSig, LpsType, ParamQualifier};
use lpvm::{LpsValue, LpvmInstance};
use crate::error::WasmError;
use crate::module::{SHADOW_STACK_GLOBAL_EXPORT, WasmExport};
use super::link;
use super::BrowserLpvmModule;

pub struct BrowserLpvmInstance {
    instance: WebAssembly::Instance,
    memory: Option<WebAssembly::Memory>,
    exports_obj: JsValue,
    exports: HashMap<String, WasmExport>,
    signatures: LpsModuleSig,
    shadow_stack_base: Option<i32>,
    float_mode: FloatMode,
}

impl BrowserLpvmInstance {
    pub(crate) fn new(module: &BrowserLpvmModule) -> Result<Self, WasmError> {
        let linked = link::instantiate_shader(&module.wasm_bytes)?;
        let exports_obj = Reflect::get(
            &linked.instance,
            &"exports".into(),
        ).map_err(|e| WasmError::runtime(format!("get exports: {e:?}")))?;

        Ok(Self {
            instance: linked.instance,
            memory: linked.memory,
            exports_obj,
            exports: module.exports.clone(),
            signatures: module.signatures.clone(),
            shadow_stack_base: module.shadow_stack_base,
            float_mode: module.opts.float_mode,
        })
    }

    fn prepare_call(&self) -> Result<(), WasmError> {
        if let Some(base) = self.shadow_stack_base {
            // Get the shadow stack global and reset it
            let global = Reflect::get(&self.exports_obj, &SHADOW_STACK_GLOBAL_EXPORT.into())
                .map_err(|e| WasmError::runtime(format!("get shadow stack: {e:?}")))?;
            Reflect::set(&global, &"value".into(), &JsValue::from(base))
                .map_err(|e| WasmError::runtime(format!("set shadow stack: {e:?}")))?;
        }
        Ok(())
    }

    /// Access the underlying WebAssembly.Instance (for direct render_frame calls).
    pub fn js_instance(&self) -> &WebAssembly::Instance {
        &self.instance
    }

    /// Access the shared memory (for pixel readback).
    pub fn js_memory(&self) -> Option<&WebAssembly::Memory> {
        self.memory.as_ref()
    }

    /// Access the exports object.
    pub fn js_exports(&self) -> &JsValue {
        &self.exports_obj
    }
}
```

Implement `LpvmInstance`:

```rust
impl LpvmInstance for BrowserLpvmInstance {
    type Error = WasmError;

    fn call(&mut self, name: &str, args: &[LpsValue]) -> Result<LpsValue, Self::Error> {
        // Same validation as rt_wasmtime: check fn_sig, reject out/inout, etc.

        let export = self.exports.get(name).cloned()
            .ok_or_else(|| WasmError::runtime(format!("function '{name}' not found")))?;

        // Get the function from exports
        let func = Reflect::get(&self.exports_obj, &name.into())
            .map_err(|e| WasmError::runtime(format!("get {name}: {e:?}")))?;
        let func: Function = func.dyn_into()
            .map_err(|_| WasmError::runtime(format!("{name} is not a function")))?;

        // Marshal args to JsValue array
        let js_args = marshal::build_js_args(&export.param_types, args, self.float_mode)?;

        self.prepare_call()?;

        // Call the function
        let result = func.apply(&JsValue::NULL, &js_args)
            .map_err(|e| WasmError::runtime(format!("WASM trap: {e:?}")))?;

        // Unmarshal result
        marshal::js_result_to_lps_value(&export.return_type, &result, self.float_mode)
    }
}
```

### 5. Create `rt_browser/marshal.rs`

Marshal `LpsValue` → JS args and JS results → `LpsValue`.

Key differences from `rt_wasmtime/marshal.rs`:
- Args are `js_sys::Array` of `JsValue` (numbers), not `Vec<wasmtime::Val>`
- Multi-value returns: browser WASM functions return a single value or
  (if multi-value) need to be handled differently. Check if the browser
  runtime returns multi-value results as JS arrays or if we need a different
  approach.

Note: Browser WebAssembly multi-value returns are not universally supported
the same way as wasmtime. For functions returning multiple values (vec2, vec3,
etc.), the WASM ABI may use multi-value returns. If the browser doesn't
support `Function.call()` with multi-value, we may need to wrap or handle
differently. For this initial implementation, focus on scalar returns and
verify multi-value behavior in the web-demo phase.

```rust
use js_sys::{Array, JsString, Number};
use wasm_bindgen::JsValue;
use lpir::FloatMode;
use lps_shared::LpsType;
use lpvm::LpsValue;
use crate::error::WasmError;

const Q16_16_SCALE: f32 = 65536.0;

pub(crate) fn build_js_args(
    param_types: &[LpsType],
    args: &[LpsValue],
    fm: FloatMode,
) -> Result<Array, WasmError> {
    let arr = Array::new();
    // VMContext i32 as first arg
    arr.push(&JsValue::from(0i32));
    for (v, ty) in args.iter().zip(param_types.iter()) {
        push_lps_value_flat(&arr, ty, v, fm)?;
    }
    Ok(arr)
}

fn push_lps_value_flat(arr: &Array, ty: &LpsType, v: &LpsValue, fm: FloatMode) -> Result<(), WasmError> {
    match (ty, v) {
        (LpsType::Float, LpsValue::F32(f)) => {
            arr.push(&encode_f32_js(*f, fm));
        }
        (LpsType::Int, LpsValue::I32(i)) => {
            arr.push(&JsValue::from(*i));
        }
        // ... (same mapping as rt_wasmtime marshal, but producing JsValue)
        _ => return Err(WasmError::runtime("type mismatch in marshal")),
    }
    Ok(())
}

fn encode_f32_js(f: f32, fm: FloatMode) -> JsValue {
    match fm {
        FloatMode::Q32 => JsValue::from((f * Q16_16_SCALE) as i32),
        FloatMode::F32 => JsValue::from_f64(f as f64),
    }
}
```

Result unmarshaling handles the JS return value. For scalar returns, the
result is a single Number. For multi-value, it may be an Array (depends on
browser behavior — test in Phase 4).

### 6. Update `lib.rs` — add `rt_browser` module

Remove the stub from Phase 2 and add the real module:

```rust
#[cfg(not(target_arch = "wasm32"))]
pub mod rt_wasmtime;
#[cfg(target_arch = "wasm32")]
pub mod rt_browser;
```

## Validate

This phase can't be fully tested on host (it targets `wasm32`). Validate:

```bash
# Host still works
cargo check -p lpvm-wasm
cargo test -p lpvm-wasm

# Browser build compiles
cargo check -p lpvm-wasm --target wasm32-unknown-unknown
```

Full browser testing happens in Phase 4 (web-demo).
