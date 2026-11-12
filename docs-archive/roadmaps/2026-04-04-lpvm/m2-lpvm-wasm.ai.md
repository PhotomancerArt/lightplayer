# M2: `lpvm-wasm`

## Goal

Build the WASM backend for LPVM. Done first because the WASM runtime model is
the strictest — wasmtime and browser APIs constrain the trait design.

## Naming / paths

- **Shader layer** uses **`lps-*`** (e.g., `lps-frontend` for GLSL → LPIR).
- **LPVM backends** use **`lpvm-*`** (e.g., `lpvm-wasm` for LPIR → WASM).

**Actual location:** `lp-shader/lpvm-wasm/` (not `lpvm/lpvm-wasm/` as originally
planned — the `lpvm/` directory structure was kept flat).

## Context for Agents

### What was built

The WASM backend now exists and has two runtime implementations:

1. **Emission** (always, `no_std` + alloc): LPIR → WASM bytes via `wasm-encoder`
2. **Runtime** (target-selected): 
   - **Native** (`!wasm32`): `rt_wasmtime/` — wasmtime implementation of LPVM traits
   - **Browser** (`wasm32`): `rt_browser/` — `js_sys::WebAssembly` implementation

### Current API

**Entry point (emission only):**
```rust
pub fn compile_lpir(ir: &IrModule, sig: &LpsModuleSig, opts: &WasmOptions) -> Result<WasmArtifact, WasmError>
```

**Runtime types (native):**
```rust
pub struct WasmLpvmEngine { /* wasmtime Engine + options */ }
pub struct WasmLpvmModule { /* bytes + metadata + parsed Module */ }
pub struct WasmLpvmInstance { /* Store + Instance + memory */ }

impl LpvmEngine for WasmLpvmEngine { type Module = WasmLpvmModule; ... }
impl LpvmModule for WasmLpvmModule { type Instance = WasmLpvmInstance; ... }
impl LpvmInstance for WasmLpvmInstance { ... }
```

**Runtime types (browser/wasm32):**
```rust
pub struct BrowserLpvmEngine { /* builtins exports */ }
pub struct BrowserLpvmModule { /* js_sys::WebAssembly::Module */ }
pub struct BrowserLpvmInstance { /* Instance + exports */ }

impl LpvmEngine for BrowserLpvmEngine { type Module = BrowserLpvmModule; ... }
impl LpvmModule for BrowserLpvmModule { type Instance = BrowserLpvmInstance; ... }
impl LpvmInstance for BrowserLpvmInstance { ... }
```

### Builtins linking

`lps-builtins` is a direct Rust dependency. **No separate `.wasm` file.**

- **Native**: Uses `wasmtime::Func::new()` with native builtin function pointers
- **Browser**: Builtin exports from the main WASM binary passed via `lpvm_init_exports()`

### WASM constraints vs traits

The trait design accommodates:
- VMContext passed as first I32 parameter to all shader functions
- Shadow stack global reset before each call
- Fuel limits via wasmtime fuel consumption
- Builtins linked by name from `lps-builtin-ids` registry

## What Was Built

### Crate location

`lp-shader/lpvm-wasm/`

### `Cargo.toml` structure

```toml
[package]
name = "lpvm-wasm"

[features]
default = []
# No "runtime" feature — runtime always included, target-selected

[dependencies]
# Core dependencies (always, no_std + alloc)
lpir = { path = "../lpir" }
lps-shared = { path = "../lps-shared" }
lpvm = { path = "../lpvm" }
wasm-encoder = "0.245"
lps-builtin-ids = { path = "../lps-builtin-ids" }

# Runtime: wasmtime (native targets only)
wasmtime = { version = "42", optional = false }  # Actually compiled via target cfgs

# Runtime: js_sys (wasm32 targets only)
wasm-bindgen = { version = "0.2", optional = false }  # Actually compiled via target cfgs
js-sys = { version = "0.3", optional = false }
web-sys = { version = "0.3", features = ["console"], optional = false }
```

### Module structure

```
lpvm-wasm/
├── Cargo.toml
└── src/
    ├── lib.rs              # Target-gated exports (rt_wasmtime / rt_browser)
    ├── compile.rs          # LPIR → WASM bytes (emit_lpir)
    ├── emit/               # Emission submodules
    │   ├── mod.rs          # Module emission orchestration
    │   ├── control.rs      # Control flow
    │   ├── func.rs         # Function encoding
    │   ├── imports.rs      # Import filtering
    │   ├── memory.rs       # Shadow stack
    │   ├── ops.rs          # LPIR → WASM instructions
    │   └── q32.rs          # Q32 operations
    ├── module.rs           # WasmModule, WasmExport
    ├── options.rs          # WasmOptions
    ├── error.rs            # WasmError
    ├── rt_wasmtime/        # Native runtime (#[cfg(not(target_arch = "wasm32"))])
    │   ├── mod.rs
    │   ├── engine.rs       # WasmLpvmEngine
    │   ├── instance.rs     # WasmLpvmInstance
    │   ├── link.rs         # Native builtin linking
    │   └── marshal.rs      # Value marshaling
    └── rt_browser/         # Browser runtime (#[cfg(target_arch = "wasm32")])
        ├── mod.rs
        ├── engine.rs       # BrowserLpvmEngine
        ├── instance.rs     # BrowserLpvmInstance
        ├── link.rs         # JS builtin linking
        └── marshal.rs      # LpsValue ↔ JsValue
```

### Emission

Core entry: **`compile_lpir(ir, sig, opts) -> WasmArtifact`**.

`WasmArtifact` contains:
- `wasm_module(): &WasmModule` — bytes, exports, shadow_stack_base
- `signatures(): &LpsModuleSig` — metadata

### Runtime (target-selected, no feature flag)

**Native (`!wasm32`):**
- `WasmLpvmEngine::new(opts)` creates engine with native builtins linked
- `compile()` produces `WasmLpvmModule` (implements `LpvmModule`)
- `instantiate()` produces `WasmLpvmInstance` (implements `LpvmInstance`)

**Browser (`wasm32`):**
- `BrowserLpvmEngine::new(builtin_exports)` stores builtin function exports
- Same trait interface, different implementation using `js_sys::WebAssembly`

### Unit tests

Tests in `lpvm-wasm/tests/`:
- `compile_roundtrip.rs` — LPIR → WASM bytes validation
- `runtime_lpvm_call.rs` — wasmtime instantiation and trait-based calling
- `runtime_builtin_sin.rs` — builtin linking validation

## What To Do (Deferred or Future)

- `lpvm-wasm` is **done** — both native and browser runtimes work
- `web-demo` integration was part of Stage II (separate plan)
- Filetest migration is M5 (separate milestone)

## What NOT To Do

- Do NOT add a `runtime` feature flag — use target-based cfg instead
- Do NOT expect `LpvmMemory` trait — it was not needed; memory is internal to each backend
- Do NOT use old paths like `lpvm/lpvm-wasm/` — actual path is `lp-shader/lpvm-wasm/`

## Done When

- [x] `lpvm-wasm` emission builds and produces valid WASM
- [x] `rt_wasmtime` implements LPVM traits with wasmtime
- [x] `rt_browser` implements LPVM traits for wasm32 target
- [x] Tests pass: emission roundtrip, runtime calls, builtin linking
- [x] Target-based gating works (`!wasm32` vs `wasm32`)
- [x] `lps-builtins` linked directly (no separate `.wasm` file)
