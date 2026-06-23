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
`www/rainbow-default.glsl` from `examples/basic/shader.glsl`.

## Run

```bash
just web-demo
```

Open the URL printed by miniserve (default `http://127.0.0.1:2812`). The editor compiles on idle;
`render_frame` drives the texture (shader entry point is **`vec4 render(vec2 fragCoord, vec2 outputSize, float time)`**).

## Deploy

The existing `just web-demo-deploy` recipe still deploys the demo to the
`gh-pages` branch. The `demo.lightplayer.app` channel uses a clean staged
artifact instead:

```bash
just web-demo-deploy-dir demo target/pages/web-demo demo.lightplayer.app
just web-demo-smoke target/pages/web-demo
```

The staged artifact includes `version.json`, `.nojekyll`, and `CNAME`. Manual
deployment to the demo Pages repository runs through the `Deploy Pages Channel`
workflow. See `docs/deploy/studio-pages.md` for DNS and GitHub Pages setup.

## Linear memory

Shader memory comes from the compiled module’s `env.memory` import. The demo grows it as needed for
the pixel buffer (see `ensureWasmMemoryForPixelBuffer` in `www/index.html`).

## Layout

- `src/lib.rs` — wasm-bindgen exports (`lpvm_init_exports`, `compile_shader`, `render_frame`, …)
- `www/index.html` — UI and render loop
- `www/rainbow-default.glsl` — default shader (synced by `web-demo-build`)
- `www/pkg/` — generated; gitignored
