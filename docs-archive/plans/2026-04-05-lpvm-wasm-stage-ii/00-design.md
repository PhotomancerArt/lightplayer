# M2 Stage II: Design Overview

## Scope

1. Direct-link `lps-builtins` into `lpvm-wasm` (replace filesystem `.wasm` loading)
2. Add browser runtime backend (`rt_browser`) for `wasm32` targets
3. Refactor host runtime (`rt_wasmtime`) for native builtin linking
4. Update `web-demo` to use `lpvm-wasm` end-to-end
5. Move `ensure_builtins_referenced()` into `lps-builtins`

## File Structure

```
lp-shader/
├── lps-builtins/
│   └── src/
│       ├── builtin_refs.rs          # MOVE: DCE prevention (from lps-builtins-wasm, auto-generated)
│       └── lib.rs                   # UPDATE: pub mod builtin_refs
│
├── lpvm-wasm/
│   ├── Cargo.toml                   # UPDATE: drop runtime feature, add deps, target-specific deps
│   └── src/
│       ├── lib.rs                   # UPDATE: cfg-gate rt_wasmtime / rt_browser
│       ├── compile.rs               # (existing, unchanged)
│       ├── emit/                    # (existing, unchanged)
│       ├── error.rs                 # UPDATE: browser error variants
│       ├── module.rs                # (existing, unchanged)
│       ├── options.rs               # (existing, unchanged)
│       ├── rt_wasmtime/
│       │   ├── mod.rs               # RENAME from runtime/mod.rs
│       │   ├── engine.rs            # UPDATE: native builtin linking via Func::new
│       │   ├── instance.rs          # RENAME from runtime/instance.rs
│       │   ├── link.rs              # UPDATE: remove filesystem loading, link native builtins
│       │   └── marshal.rs           # RENAME from runtime/marshal.rs
│       └── rt_browser/
│           ├── mod.rs               # NEW
│           ├── engine.rs            # NEW: BrowserLpvmEngine, BrowserLpvmModule
│           ├── instance.rs          # NEW: BrowserLpvmInstance
│           ├── link.rs              # NEW: js_sys shader instantiation with builtin imports
│           └── marshal.rs           # NEW: LpsValue ↔ JsValue marshaling
│
└── lp-app/
    └── web-demo/
        ├── Cargo.toml               # UPDATE: lpvm-wasm + lps-frontend, drop lps-wasm
        ├── src/
        │   └── lib.rs               # UPDATE: full pipeline via wasm-bindgen
        └── www/
            └── index.html           # UPDATE: thin JS — init, pass exports, raf, canvas blit

```

## Architecture

```
                        lpvm-wasm
                    ┌───────────────────────────────────┐
  GLSL source       │  compile_lpir()                    │
       │            │  (LPIR → WASM bytes, always)       │
       ▼            │                                    │
  lps-frontend      ├───────────────┬───────────────────┤
  (GLSL → LPIR)     │ rt_wasmtime   │ rt_browser         │
                    │ (!wasm32)     │ (wasm32)           │
                    │               │                    │
                    │ Func::new()   │ init_exports()     │
                    │ calls native  │ stores self-exports│
                    │ lps-builtins  │ as WebAssembly.Fn  │
                    │               │                    │
                    │ wasmtime      │ js_sys::WebAssembly │
                    │ Store/Instance│ Module/Instance    │
                    └───────┬───────┴────────┬──────────┘
                            │                │
                     LpvmEngine        LpvmEngine
                     LpvmModule        LpvmModule
                     LpvmInstance      LpvmInstance
                            │                │
                            ▼                ▼
                    host tests/apps    web-demo (browser)

  lps-builtins: shared dependency of lpvm-wasm
  ├── #[no_mangle] pub extern "C" fn __lps_sin_q32(i32) -> i32
  ├── ... (~100 builtins)
  └── ensure_builtins_referenced()  ← DCE prevention
```

## Key Decisions

- **Direct linking**: `lps-builtins` is a Rust dependency; no separate `.wasm` binary
- **No feature flag**: runtime always included, target-selects backend
- **Parallel dirs**: `rt_wasmtime/` and `rt_browser/`, cfg-gated
- **Full Rust runtime in web-demo**: compile + instantiate + render_frame in Rust
- **DCE prevention in `lps-builtins`**: `ensure_builtins_referenced()` moves upstream
- **`lps-builtins-wasm`**: kept in `legacy/` until M5 filetests migration
