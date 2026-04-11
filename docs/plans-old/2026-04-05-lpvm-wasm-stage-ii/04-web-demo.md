# Phase 4: Update web-demo

## Scope

Switch web-demo from `lps-wasm` (legacy) to `lpvm-wasm` + `lps-frontend`.
Full GLSL → LPIR → WASM → instantiate → render_frame pipeline in Rust.
JS becomes thin: init, pass exports, requestAnimationFrame, canvas blit.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment.

## Implementation

### 1. Update `web-demo/Cargo.toml`

```toml
[package]
name = "web-demo"
version.workspace = true
authors.workspace = true
edition.workspace = true
license.workspace = true
rust-version.workspace = true
publish = false
description = "GLSL → WASM browser demo using lpvm-wasm browser runtime"

[lints]
workspace = true

[lib]
crate-type = ["cdylib"]

[dependencies]
lpvm-wasm = { path = "../../lp-shader/lpvm-wasm" }
lps-frontend = { path = "../../lp-shader/lps-frontend" }
lpvm = { path = "../../lp-shader/lpvm" }
wasm-bindgen = "0.2"
js-sys = "0.3"
```

Remove `lps-wasm` dependency.

### 2. Rewrite `web-demo/src/lib.rs`

The new lib.rs exposes:
- `compile_shader(source: &str) -> Result<(), String>` — compiles GLSL
  through the full pipeline and stores the instance.
- `render_frame(width: u32, height: u32, time_q32: i32, out_ptr: u32)` —
  calls `render_frame` on the shader instance.
- `get_memory() -> JsValue` — returns the shared memory for pixel readback.

```rust
use wasm_bindgen::prelude::*;
use js_sys::Reflect;
use lps_frontend::{compile, lower};
use lpvm::LpvmEngine;
use lpvm_wasm::rt_browser::{BrowserLpvmEngine, BrowserLpvmModule, BrowserLpvmInstance};
use lpvm_wasm::{FloatMode, WasmOptions};

use std::cell::RefCell;

thread_local! {
    static ENGINE: RefCell<Option<BrowserLpvmEngine>> = RefCell::new(None);
    static INSTANCE: RefCell<Option<BrowserLpvmInstance>> = RefCell::new(None);
}

/// Initialize the LPVM engine. Must be called after lpvm_init_exports.
#[wasm_bindgen]
pub fn init_engine() {
    let opts = WasmOptions { float_mode: FloatMode::Q32 };
    ENGINE.with(|e| *e.borrow_mut() = Some(BrowserLpvmEngine::new(opts)));
}

/// Compile GLSL source and instantiate the shader.
#[wasm_bindgen]
pub fn compile_shader(source: &str) -> Result<(), String> {
    let naga = compile(source).map_err(|e| format!("parse: {e}"))?;
    let (ir, meta) = lower(&naga).map_err(|e| format!("lower: {e}"))?;

    ENGINE.with(|e| {
        let engine = e.borrow();
        let engine = engine.as_ref().ok_or("engine not initialized")?;
        let module = engine.compile(&ir, &meta).map_err(|e| format!("compile: {e}"))?;

        use lpvm::LpvmModule;
        let instance = module.instantiate().map_err(|e| format!("instantiate: {e}"))?;
        INSTANCE.with(|i| *i.borrow_mut() = Some(instance));
        Ok(())
    })
}

/// Call render_frame on the current shader instance.
/// Returns false if no shader is loaded.
#[wasm_bindgen]
pub fn render_frame(width: i32, height: i32, time_q32: i32, out_ptr: i32) -> Result<(), String> {
    INSTANCE.with(|i| {
        let i = i.borrow();
        let instance = i.as_ref().ok_or("no shader loaded")?;

        // Call render_frame directly via JS exports for performance
        // (avoid LpsValue marshaling overhead for the hot path)
        let exports = instance.js_exports();
        let func = Reflect::get(exports, &"render_frame".into())
            .map_err(|e| format!("get render_frame: {e:?}"))?;
        let func: js_sys::Function = func.dyn_into()
            .map_err(|_| "render_frame is not a function".to_string())?;

        let args = js_sys::Array::new();
        args.push(&JsValue::from(width));
        args.push(&JsValue::from(height));
        args.push(&JsValue::from(time_q32));
        args.push(&JsValue::from(out_ptr));

        func.apply(&JsValue::NULL, &args)
            .map_err(|e| format!("render_frame trap: {e:?}"))?;
        Ok(())
    })
}

/// Get the shared WebAssembly.Memory for pixel readback.
#[wasm_bindgen]
pub fn get_shader_memory() -> JsValue {
    INSTANCE.with(|i| {
        let i = i.borrow();
        match i.as_ref().and_then(|inst| inst.js_memory()) {
            Some(mem) => mem.into(),
            None => JsValue::NULL,
        }
    })
}
```

Note: `render_frame` bypasses the `LpvmInstance::call` trait for performance
— calling it via `LpsValue` marshaling every frame would add overhead. The
JS export path is a direct WASM function call. The trait is validated by
`compile_shader` / `instantiate`.

For full trait validation, add a `call_function` export that goes through
the trait:

```rust
#[wasm_bindgen]
pub fn call_function(name: &str, args_json: &str) -> Result<String, String> {
    // Parse args from JSON, call via LpvmInstance::call, return result as JSON
    // This validates the full trait chain
}
```

### 3. Update `web-demo/www/index.html`

Major changes:
- Remove `fetch('builtins.wasm')` and builtins instantiation JS.
- Remove `compileShader` JS function that does `WebAssembly.Module/Instance`.
- Replace with calls to Rust exports.

Key JS flow after init:

```js
import init, { lpvm_init_exports, init_engine, compile_shader, render_frame, get_shader_memory }
  from './web_demo.js';

async function main() {
    const wasm = await init();

    // Pass our own exports to lpvm-wasm for builtin resolution
    // wasm-bindgen may require accessing the underlying instance.
    // The generated JS module typically has a `__wbg_get_imports` or
    // the raw instance is internal. We need to figure out how to
    // get instance.exports from wasm-bindgen's generated code.
    //
    // Option A: Modify wasm-bindgen output to expose instance
    // Option B: Use a small JS shim that captures the instance
    // Option C: Pass exports individually
    //
    // Practical approach: wasm-bindgen generates an `__wbindgen_init_externref_table`
    // and other internals. The instance is in a closure. We may need
    // a wrapper:

    // After init(), wasm-bindgen has instantiated the module.
    // We can access builtin functions through the module's exports:
    lpvm_init_exports(wasm.__wbg_instance.exports);
    // ^^ This may not work directly. See note below.

    init_engine();

    // ... editor setup, etc.

    compile_shader(editor.getValue());

    function renderLoop(timestamp) {
        requestAnimationFrame(renderLoop);
        const timeSec = timestamp / 1000;
        const q32Time = Math.round(timeSec * 65536);
        try {
            render_frame(texSize, texSize, q32Time, PIXEL_BUF_OFFSET);
        } catch (e) {
            setError('Runtime: ' + e);
            return;
        }
        // Read pixels from shared memory
        const memory = get_shader_memory();
        if (memory) {
            const src = new Uint8Array(memory.buffer, PIXEL_BUF_OFFSET, pixelBufSize());
            imageData.data.set(src);
            ctx.putImageData(imageData, 0, 0);
        }
    }
    requestAnimationFrame(renderLoop);
}
```

**Important: wasm-bindgen instance.exports access.** The generated JS from
wasm-bindgen doesn't expose `instance.exports` directly. Approaches:

1. **Custom JS wrapper**: After `init()`, use a small JS function that was
   set up to capture the instance. wasm-bindgen's `--target web` output
   stores the instance internally. We can monkey-patch or wrap the init.

2. **wasm-pack with custom init**: Modify the init sequence to capture and
   pass exports.

3. **Direct CDN/manual init**: Skip wasm-bindgen's `init()` and do manual
   `WebAssembly.instantiate` with our own imports object, then call
   `lpvm_init_exports` with the raw instance exports. This gives full
   control but loses wasm-bindgen's import setup.

The practical solution during implementation: inspect the wasm-bindgen
output JS, find where the instance is stored, and extract exports from it.
If wasm-bindgen stores it in a module-scoped variable (common pattern),
add a tiny JS getter.

### 4. Build and test

```bash
# Build web-demo
cd lp-app/web-demo
wasm-pack build --target web --out-dir www/pkg

# Or with cargo directly:
cargo build -p web-demo --target wasm32-unknown-unknown --release

# Serve and test in browser
cd www
python3 -m http.server 8080
# Open http://localhost:8080 and verify shader renders
```

### 5. Verify

- Shader compiles without errors.
- Canvas renders the default shader.
- Changing shader source recompiles and re-renders.
- Performance is comparable to the legacy JS-runtime approach.
- No separate `builtins.wasm` fetch needed.

## Validate

```bash
# Host tests still pass
cargo check -p lpvm-wasm
cargo test -p lpvm-wasm

# web-demo builds for wasm32
cargo check -p web-demo --target wasm32-unknown-unknown

# Full browser test (manual)
# Serve lp-app/web-demo/www/ and verify rendering in browser
```
