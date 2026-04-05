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

# Full runtime tests with builtins
cargo build -p lps-builtins-wasm --target wasm32-unknown-unknown --release
cargo test -p lpvm-wasm
```

Runtime tests require `lps_builtins_wasm.wasm` at the standard path or set via `lps_BUILTINS_WASM` environment variable.

## Architecture

- **`emit/`**: LPIR → WASM emission (copied/adapted from `lps-wasm`)
- **`runtime/`**: wasmtime trait implementations (requires `runtime` feature)
- **`compile.rs`**: Main entry point for emission with metadata validation

[`lpvm`]: ../lpvm
[`LpvmEngine`]: ../lpvm/trait.LpvmEngine.html
[`LpvmModule`]: ../lpvm/trait.LpvmModule.html
[`LpvmInstance`]: ../lpvm/trait.LpvmInstance.html
[`WasmLpvmEngine`]: src/runtime/engine/struct.WasmLpvmEngine.html
[`WasmLpvmModule`]: src/runtime/engine/struct.WasmLpvmModule.html
[`WasmLpvmInstance`]: src/runtime/instance/struct.WasmLpvmInstance.html
