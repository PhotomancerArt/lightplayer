# lpvm-wasm

LPIR to WebAssembly emission and wasmtime runtime for LPVM.

This crate compiles LightPlayer IR (LPIR) to WASM bytecode and provides an optional wasmtime runtime implementing the [`lpvm`] traits.

## Features

- **`runtime`** (enabled by default): Enables the wasmtime-based runtime with full trait implementations:
  - [`WasmLpvmEngine`] implements [`LpvmEngine`]
  - [`WasmLpvmModule`] implements [`LpvmModule`]
  - [`WasmLpvmInstance`] implements [`LpvmInstance`]

  Disable default features to use emission only:
  ```toml
  [dependencies]
  lpvm-wasm = { path = "../lpvm-wasm", default-features = false }
  ```

## Usage

### Emit-only (no_std compatible)

Compile LPIR to WASM bytes for use in browsers or other runtimes:

```rust
use lpvm_wasm::{compile_lpir, WasmOptions};
use lpir::IrModule;
use lps_shared::LpsModuleSig;

let artifact = compile_lpir(&ir_module, &metadata, &WasmOptions::default())?;
let wasm_bytes = artifact.bytes(); // &[u8] ready for wasmtime, JS, etc.
```

### With runtime (wasmtime)

Execute shaders with builtin linking:

```rust
use lpvm_wasm::runtime::WasmLpvmEngine;
use lpvm::{LpvmEngine, LpvmModule, LpvmInstance, LpsValue};

// Load builtins and create engine
let engine = WasmLpvmEngine::try_default_builtins(WasmOptions::default())?;

// Compile module
let module = engine.compile(&ir, &metadata)?;

// Execute
let mut instance = module.instantiate()?;
let result = instance.call("main", &[LpsValue::F32(1.0)])?;
```

## Testing

```bash
# Emission tests only (no wasmtime)
cargo test -p lpvm-wasm --no-default-features

# Full runtime tests with native builtins
cargo test -p lpvm-wasm
```

## Fuel Metering

Emitted modules are fuel-metered so an infinite loop in shader code
aborts cleanly instead of hanging the host — including the browser sim's
web worker (see `docs/adr/2026-07-23-sim-wasm-fuel.md`; the unit and
header contract are shared with the rv32 backend,
`docs/adr/2026-07-20-lpvm-native-fuel.md`):

- `emit/fuel.rs` inserts a check-then-decrement of the fuel low u32
  (vmctx+0) before every loop back-edge `br` and a check-only at every
  function entry. Always on (`WasmOptions::fuel`, default `true`;
  `false` is for tests/perf comparison only).
- On observing 0 the check stores `TRAP_CODE_OUT_OF_FUEL` to the vmctx
  trap slot (offset 8) and executes `unreachable` — the whole call
  unwinds to the host, which classifies by reading the slot (never the
  runtime's error message).
- Both hosts (`rt_wasmtime`, `rt_browser`) arm the header before every
  guest entry and surface `WasmError::Trap { code, invocation }`
  (structured access via `lpvm::GuestTrapError`). The synthesised render
  wrappers re-arm a per-pixel/sample tank, so a trap names the offending
  pixel. wasmtime store fuel is no longer used.
- `__lp_get_fuel` is inlined as a direct vmctx load (no import): the
  native builtin's pointer deref would be a null dereference at the wasm
  hosts' vmctx offset 0.

## Architecture

- **`emit/`**: LPIR → WASM emission (copied/adapted from earlier `lps-wasm` layout; crate is `lpvm-wasm`)
- **`runtime/`**: wasmtime trait implementations (requires `runtime` feature)
- **`compile.rs`**: Main entry point for emission with metadata validation

[`lpvm`]: ../lpvm
[`LpvmEngine`]: ../lpvm/trait.LpvmEngine.html
[`LpvmModule`]: ../lpvm/trait.LpvmModule.html
[`LpvmInstance`]: ../lpvm/trait.LpvmInstance.html
[`WasmLpvmEngine`]: src/runtime/engine/struct.WasmLpvmEngine.html
[`WasmLpvmModule`]: src/runtime/engine/struct.WasmLpvmModule.html
[`WasmLpvmInstance`]: src/runtime/instance/struct.WasmLpvmInstance.html
