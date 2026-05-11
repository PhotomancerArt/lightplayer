## Phase 4: Runtime Infrastructure (Engine/Module/Instance Skeletons)

### Scope

Add `runtime` feature code skeletons. Create `WasmEngine`, runtime `WasmModule`
(wraps wasmtime `Module`), and `WasmInstance` (wraps wasmtime `Store` + `Instance`)
without trait implementations yet.

### Implementation Details

**runtime/engine.rs:**

```rust
use alloc::vec::Vec;
use wasmtime::{Config, Engine};

/// WASM engine with shared wasmtime Engine and builtins.
pub struct WasmEngine {
    engine: Engine,
    builtins_bytes: Vec<u8>,
}

impl WasmEngine {
    /// Create a new WASM engine with default configuration.
    pub fn new(builtins_bytes: Vec<u8>) -> Result<Self, WasmError> {
        let mut config = Config::new();
        config.consume_fuel(true);
        let engine = Engine::new(&config)
            .map_err(|e| WasmError::Instantiation(format!("engine creation: {e}")))?;
        Ok(Self {
            engine,
            builtins_bytes,
        })
    }

    /// Get reference to the wasmtime Engine.
    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    /// Get builtins WASM bytes.
    pub fn builtins_bytes(&self) -> &[u8] {
        &self.builtins_bytes
    }
}
```

**runtime/module.rs (new file):**

```rust
use wasmtime::Module;

/// Runtime WASM module - wraps a parsed wasmtime Module with metadata.
pub struct RuntimeWasmModule {
    module: Module,
    // TODO: carry exports, shadow_stack_base, metadata for instantiation
}

impl RuntimeWasmModule {
    pub fn from_wasmtime_module(module: Module) -> Self {
        Self { module }
    }

    pub fn module(&self) -> &Module {
        &self.module
    }
}
```

**runtime/instance.rs:**

```rust
use wasmtime::{Instance, Store};

/// WASM execution instance with mutable state.
pub struct WasmInstance {
    store: Store<()>,
    instance: Instance,
    // TODO: memory handle, shadow stack base, fuel config
}

impl WasmInstance {
    /// Create from linked instance.
    pub fn new(store: Store<()>, instance: Instance) -> Self {
        Self { store, instance }
    }
}
```

**runtime/mod.rs updates:**

```rust
pub mod engine;
pub mod instance;
pub mod module;

pub use engine::WasmEngine;
pub use instance::WasmInstance;
pub use module::RuntimeWasmModule;
```

**Update Cargo.toml:**

Ensure `wasmtime` dependency is present with `runtime` feature.

### Notes

- `Store<()>` — empty user data for now. May add state later.
- Engine creation is expensive — that's why it's shared in `WasmEngine`.
- Module parsing happens in `compile()` (next phase).
- Linking builtins + shader happens in `instantiate()` (next phase).

### Validate

```bash
cargo check -p lpvm-wasm --features runtime 2>&1 | head -30
```

Fix any wasmtime API mismatches (version 42 used elsewhere in workspace).
