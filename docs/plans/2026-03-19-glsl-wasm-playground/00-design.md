# GLSL → WASM Web Demo: Design

## Scope

Build a browser-based demo that compiles GLSL to WASM and renders shader output to a canvas. The GLSL compiler itself runs as WASM in the browser (no server). Target: rainbow.shader renders in a browser.

## File structure

```
lp-app/
└── web-demo/
    ├── Cargo.toml              # NEW: wasm-pack cdylib, deps: lp-glsl-frontend, lp-glsl-wasm, wasm-bindgen
    ├── src/
    │   └── lib.rs              # NEW: wasm-bindgen API: compile_glsl(source) → Vec<u8> or error
    └── www/
        └── index.html          # NEW: textarea, canvas, JS glue, render loop

scripts/
└── build-builtins.sh           # EXISTS: already builds lp_glsl_builtins_wasm.wasm

justfile                        # UPDATE: add web-demo-build and web-demo recipes

Cargo.toml                      # UPDATE: add lp-app/web-demo to members (not default-members)
```

## Architecture

```
                  ┌─────────────────────────┐
                  │       index.html         │
                  │                          │
                  │  textarea ──onChange──┐   │
                  │                      │   │
                  │  canvas  ◄─ rAF loop │   │
                  │                      │   │
                  │  error panel         │   │
                  └──────────────────────┼───┘
                                         │
                    JS glue              ▼
            ┌────────────────────────────────────────┐
            │                                        │
            │  1. compile_glsl(source) → wasm bytes  │
            │     (calls into compiler.wasm)          │
            │                                        │
            │  2. WebAssembly.instantiate(            │
            │       shaderBytes,                      │
            │       { builtins: builtinsExports,      │
            │         env: { memory: sharedMemory } } │
            │     )                                   │
            │                                        │
            │  3. for each pixel:                     │
            │       shader.main(fx,fy,sx,sy,t)        │
            │         → [r, g, b, a] (Q32 i32s)      │
            │       convert to 0-255 RGBA             │
            │       write to ImageData                │
            │                                        │
            │  4. putImageData → canvas               │
            └──────────┬───────────────┬─────────────┘
                       │               │
              ┌────────▼──────┐  ┌─────▼──────────────┐
              │ compiler.wasm │  │  builtins.wasm      │
              │               │  │                     │
              │ lp-glsl-      │  │ lp-glsl-builtins-   │
              │ frontend      │  │ wasm (pre-built)    │
              │    +          │  │                     │
              │ lp-glsl-wasm  │  │ __lp_q32_sin, etc. │
              │               │  │                     │
              │ via wasm-pack │  │ shares env.memory   │
              └───────────────┘  └─────────────────────┘
```

## Components

### 1. `lp-app/web-demo` crate (Rust → WASM via wasm-pack)

A thin `cdylib` crate exposing wasm-bindgen functions to JS.

```rust
#[wasm_bindgen]
pub fn compile_glsl(source: &str) -> Result<Vec<u8>, String> {
    let options = WasmOptions {
        float_mode: FloatMode::Q32,
        ..Default::default()
    };
    match glsl_wasm(source, options) {
        Ok(module) => Ok(module.bytes),
        Err(diagnostics) => Err(format!("{}", diagnostics)),
    }
}
```

Dependencies: `lp-glsl-frontend`, `lp-glsl-wasm`, `wasm-bindgen`. No Cranelift.

### 2. `index.html` (single-file web page)

- **Textarea**: Pre-loaded with rainbow.shader source. Auto-compiles on change (debounced).
- **Canvas**: 64×64 pixel grid, scaled up for visibility.
- **Error panel**: Shows compilation errors. While user edits, keeps rendering last successful shader.
- **JS glue**: Loads compiler WASM (wasm-pack output) and builtins WASM (fetched as separate file). Handles compile → instantiate → render loop.

### 3. JS rendering loop

```
requestAnimationFrame(render)

render(timestamp):
  time = timestamp / 1000.0  (seconds)
  q32Time = Math.round(time * 65536)

  for y in 0..64:
    for x in 0..64:
      [r, g, b, a] = shader.main(x<<16, y<<16, 64<<16, 64<<16, q32Time)
      // Q32 → 0-255: clamp(value >> 8, 0, 255) since Q16.16 * 256 = >> 8
      imageData[offset] = clamp(r >> 8, 0, 255)
      ...

  ctx.putImageData(imageData, 0, 0)
```

Multi-value returns from WASM to JS (supported in all modern browsers).

### 4. Builtins linking in the browser

Same pattern as wasmtime tests:

```js
const memory = new WebAssembly.Memory({ initial: 1 });
const builtins = await WebAssembly.instantiate(builtinsWasm, { env: { memory } });
const shader = await WebAssembly.instantiate(shaderWasm, {
    builtins: builtins.instance.exports,
    env: { memory }
});
```

### 5. Build pipeline

1. `cargo build -p lp-glsl-builtins-wasm --target wasm32-unknown-unknown --release`
2. `wasm-pack build lp-app/web-demo/ --target web`
3. Copy `builtins.wasm` to `lp-app/web-demo/www/`
4. Copy wasm-pack output (`pkg/`) to `lp-app/web-demo/www/pkg/`
5. Serve `lp-app/web-demo/www/` via `miniserve`

Justfile recipes: `web-demo-build`, `web-demo`.

## Key decisions

- **Multi-value returns**: Rely on browser support (option a). No codegen changes.
- **Rendering loop**: JS pixel loop (option a). Rust render loop deferred.
- **Crate location**: `lp-app/web-demo/`.
- **Default shader**: rainbow.shader pre-loaded.
- **Auto-compile**: On change, debounced. Keep rendering last successful shader.
- **Builtins distribution**: Fetched as separate file.
- **Dev server**: `miniserve` (Rust-native).
