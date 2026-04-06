# GLSL → WASM web demo

In-browser pipeline: **GLSL** → `lps-frontend` (Naga) → LPIR → **`lpvm-wasm`** → WASM. Builtin implementations are **Rust from `lps-builtins`**, linked into the same `web_demo.wasm` as the compiler (no separate `builtins.wasm`).

The page calls `lpvm_init_exports` / `init_engine`, then `compile_shader` / `render_frame` / `get_shader_memory` from the wasm-bindgen bundle (`www/pkg/`).

## Prerequisites

- Rust toolchain with `wasm32-unknown-unknown` (`rustup target add wasm32-unknown-unknown`)
- [wasm-bindgen-cli](https://rustwasm.github.io/wasm-bindgen/) matching the crate version (see
  `Cargo.toml`):

  ```bash
  cargo install wasm-bindgen-cli --version 0.2.114
  ```

- [miniserve](https://github.com/svenstaro/miniserve) — `just web-demo` runs
  `cargo install miniserve` automatically if it is not on `PATH`.

## Build

From the workspace root:

```bash
just web-demo-build
```

This builds `web-demo` for wasm32 (release), runs `wasm-bindgen` into `www/pkg/`, and refreshes
`www/rainbow-default.glsl` from `examples/basic/src/rainbow.shader/main.glsl`.

## Run

```bash
just web-demo
```

Open the URL printed by miniserve (default `http://127.0.0.1:2812`). The editor compiles on idle;
`render_frame` drives the texture (shader entry point is **`vec4 render(vec2 fragCoord, vec2 outputSize, float time)`**).

## Linear memory

Shader memory comes from the compiled module’s `env.memory` import. The demo grows it as needed for
the pixel buffer (see `ensureWasmMemoryForPixelBuffer` in `www/index.html`).

## Layout

- `src/lib.rs` — wasm-bindgen exports (`lpvm_init_exports`, `compile_shader`, `render_frame`, …)
- `www/index.html` — UI and render loop
- `www/rainbow-default.glsl` — default shader (synced by `web-demo-build`)
- `www/pkg/` — generated; gitignored
