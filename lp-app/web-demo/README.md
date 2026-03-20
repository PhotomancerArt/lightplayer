# GLSL → WASM web demo

In-browser GLSL compiler (`lp-glsl-frontend` + `lp-glsl-wasm`) and rainbow-style shader linked against `lp_glsl_builtins_wasm.wasm`, same pattern as wasmtime filetests.

## Prerequisites

- Rust toolchain with `wasm32-unknown-unknown` (`rustup target add wasm32-unknown-unknown`)
- [wasm-bindgen-cli](https://rustwasm.github.io/wasm-bindgen/) matching the crate version (see `Cargo.toml`):

  ```bash
  cargo install wasm-bindgen-cli --version 0.2.114
  ```

- [miniserve](https://github.com/svenstaro/miniserve) — `just web-demo` runs `cargo install miniserve` automatically if it is not on `PATH`.

## Build

From the workspace root:

```bash
just web-demo-build
```

This builds:

- `lp-glsl-builtins-wasm` → `www/builtins.wasm`
- `web-demo` for wasm32 → `wasm-bindgen` → `www/pkg/`

## Run

```bash
just web-demo
```

Open the URL printed by miniserve (default `http://127.0.0.1:2812`, a WS2812-friendly port number). The page loads the compiler WASM, fetches builtins, compiles the textarea source, and runs `main` per pixel on a 64×64 canvas.

## Shared linear memory

`www/index.html` creates `WebAssembly.Memory` with **`initial: 17` pages** so it satisfies `lp_glsl_builtins_wasm.wasm`’s `env.memory` import (Rust/LLVM currently asks for 17 pages minimum). The shader module only needs 1 page; the larger requirement comes from the builtins artifact. If linking fails with “smaller than the declared initial of N”, raise `initial` to at least `N` or re-check the builtins module with `wasm-tools print builtins.wasm | grep memory`.

## Layout

- `src/lib.rs` — `compile_glsl` wasm-bindgen export
- `www/index.html` — UI, linking, render loop
- `www/rainbow-default.glsl` — default shader (overwritten from `examples/basic/.../main.glsl` by `web-demo-build` to stay in sync)
- `www/pkg/` — generated; gitignored
