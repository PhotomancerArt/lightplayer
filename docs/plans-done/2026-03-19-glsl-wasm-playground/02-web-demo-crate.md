# Phase 2: Create web-demo crate with wasm-bindgen API

## Scope

Create `lp-app/web-demo/`, a wasm-pack project that exposes `compile_glsl(source) → Vec<u8>` to
JavaScript via wasm-bindgen. Verify `wasm-pack build` produces working output.

## Code organization reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment.

## Implementation

### 1. Install wasm-pack

```bash
cargo install wasm-pack
```

### 2. Create crate directory and Cargo.toml

Create `lp-app/web-demo/Cargo.toml`:

```toml
[package]
name = "web-demo"
version.workspace = true
edition.workspace = true
license.workspace = true
publish = false
description = "GLSL → WASM browser demo (compiler runs in-browser)"

[lints]
workspace = true

[lib]
crate-type = ["cdylib"]

[dependencies]
lp-glsl-frontend = { path = "../../lp-shader/lp-glsl-frontend" }
lp-glsl-wasm = { path = "../../lp-shader/lp-glsl-wasm" }
wasm-bindgen = "0.2"
```

Notes:

- `cdylib` is required by wasm-pack.
- No `#![no_std]` — this crate uses wasm-bindgen which requires std (the allocator + panic handler
  come from wasm-bindgen's runtime).
- Depends only on `lp-glsl-frontend` and `lp-glsl-wasm`. No Cranelift.

### 3. Create src/lib.rs

```rust
use lp_glsl_wasm::{glsl_wasm, FloatMode, WasmOptions};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn compile_glsl(source: &str) -> Result<Vec<u8>, String> {
    let options = WasmOptions {
        float_mode: FloatMode::Q32,
        ..Default::default()
    };
    match glsl_wasm(source, options) {
        Ok(module) => Ok(module.bytes),
        Err(diagnostics) => Err(format!("{diagnostics}")),
    }
}
```

This is the entire Rust surface area exposed to JS. The compiler (parser + semantic analysis + WASM
codegen) runs when JS calls `compile_glsl`.

### 4. Add to workspace

Add to workspace `Cargo.toml` members (NOT default-members, since it's a wasm32 target):

```toml
members = [
    ...
    "lp-app/web-demo",
]
```

Do NOT add to `default-members`.

### 5. Handle lp-glsl-wasm no_std vs web-demo std

`lp-glsl-frontend` and `lp-glsl-wasm` are `#![no_std]` with `extern crate alloc`. When compiled as a
dependency of `web-demo` (which uses std via wasm-bindgen), the allocator is provided by
wasm-bindgen's runtime. This should just work — `alloc` types (`Vec`, `String`, `Box`) resolve to
the global allocator provided by the cdylib's std.

If `lp-glsl-frontend` or `lp-glsl-wasm` have conditional `std` features, enable them:

```toml
lp-glsl-frontend = { path = "../../lp-shader/lp-glsl-frontend", features = ["std"] }
```

But only if needed — try without first.

### 6. Build with wasm-pack

```bash
wasm-pack build lp-app/web-demo/ --target web
```

This produces `lp-app/web-demo/pkg/` with:

- `web_demo_bg.wasm` — the compiled WASM module
- `web_demo.js` — JS glue (ES module)
- `web_demo.d.ts` — TypeScript types
- `package.json`

### 7. Verify the output

Check that `pkg/` directory exists and contains the expected files:

```bash
ls lp-app/web-demo/pkg/
wc -c lp-app/web-demo/pkg/web_demo_bg.wasm  # should be reasonable size
```

## Validate

```bash
wasm-pack build lp-app/web-demo/ --target web
ls lp-app/web-demo/pkg/web_demo_bg.wasm
cargo build   # host build still works
cargo test    # no regressions
```
