## Phase 5: Implement LpvmEngine Trait

### Scope

Implement `LpvmEngine` for `WasmEngine`. This involves:
- Emitting LPIR to WASM bytes
- Parsing with wasmtime
- Linking with builtins
- Returning a type that implements `LpvmModule`

### Implementation Details

**Design challenge:** `LpvmEngine::compile()` must return a type implementing
`LpvmModule`. But we also want `WasmModule` (concrete, with `bytes()` method)
to be available. Options:

1. **Wrapper approach:** `WasmRuntimeModule` wraps both `WasmModule` (for bytes)
   and wasmtime `Module` (for instantiation). Implements `LpvmModule`.

2. **Trait object:** Return `Box<dyn LpvmModule>` — but that erases concrete
   methods like `bytes()`.

**Chosen:** Wrapper approach. `WasmRuntimeModule` is the return type.

**runtime/engine.rs additions:**

```rust
use lpir::IrModule;
use lps_shared::LpsModuleSig;
use lpvm::LpvmEngine;
use crate::{emit_module, WasmModule, WasmOptions};
use crate::runtime::module::WasmRuntimeModule;

impl LpvmEngine for WasmEngine {
    type Module = WasmRuntimeModule;
    type Error = WasmError;

    fn compile(
        &self,
        ir: &IrModule,
        meta: &LpsModuleSig,
    ) -> Result<Self::Module, Self::Error> {
        // 1. Emit to WASM bytes
        let options = WasmOptions::default(); // TODO: pass options through trait?
        let wasm_module = emit_module(ir, &options)?;

        // 2. Parse with wasmtime
        let wasmtime_module = Module::new(&self.engine, &wasm_module.bytes)
            .map_err(|e| WasmError::Instantiation(format!("module parse: {e}")))?;

        // 3. Create runtime module wrapper
        Ok(WasmRuntimeModule::new(
            wasm_module,
            wasmtime_module,
            meta.clone(),
        ))
    }
}
```

**runtime/module.rs full implementation:**

```rust
use alloc::vec::Vec;
use lps_shared::LpsModuleSig;
use wasmtime::Module;
use crate::module::WasmModule;

/// Runtime WASM module - combines emission output with wasmtime Module.
pub struct WasmRuntimeModule {
    /// Emission output (bytes, exports, shadow stack info)
    wasm_module: WasmModule,
    /// Parsed wasmtime module (ready to instantiate)
    wasmtime_module: Module,
    /// Function signatures for runtime marshaling
    signatures: LpsModuleSig,
}

impl WasmRuntimeModule {
    pub(crate) fn new(
        wasm_module: WasmModule,
        wasmtime_module: Module,
        signatures: LpsModuleSig,
    ) -> Self {
        Self {
            wasm_module,
            wasmtime_module,
            signatures,
        }
    }

    /// Access the emission output (bytes, exports, etc.)
    pub fn emission(&self) -> &WasmModule {
        &self.wasm_module
    }

    /// Access the parsed wasmtime module.
    pub(crate) fn wasmtime_module(&self) -> &Module {
        &self.wasmtime_module
    }
}

// LpvmModule implementation in next phase
```

### Notes

- `meta.clone()` — we need owned signatures. `LpsModuleSig` should be cheap to
  clone (it's just metadata).
- Options passing: The trait doesn't take options. We could:
  - Store default options in `WasmEngine`
  - Extend trait with options (consider for later)
  - Use `FloatMode` from `meta` if available there

For now, use default `WasmOptions`. The float mode could be inferred from
context if needed.

### Validate

```bash
cargo check -p lpvm-wasm --features runtime 2>&1 | head -30
```

Ensure trait implementation compiles.
