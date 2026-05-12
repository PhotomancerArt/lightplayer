## Phase 6: Implement LpvmModule and LpvmInstance Traits

### Scope

Implement `LpvmModule` for `WasmRuntimeModule` and `LpvmInstance` for
`WasmInstance`. The heavy lifting: linking builtins, shadow stack reset,
fuel management, and `LpsValue` marshaling.

### Implementation Details

**runtime/module.rs - LpvmModule impl:**

```rust
use lpvm::{LpvmModule, LpvmInstance};
use crate::runtime::instance::WasmInstance;
use crate::error::WasmError;

impl LpvmModule for WasmRuntimeModule {
    type Instance = WasmInstance;
    type Error = WasmError;

    fn signatures(&self) -> &LpsModuleSig {
        &self.signatures
    }

    fn instantiate(&self) -> Result<Self::Instance, Self::Error> {
        // 1. Create Store with fuel
        let mut store = Store::new(&self.wasmtime_module.engine(), ());
        store.set_fuel(DEFAULT_FUEL)
            .map_err(|e| WasmError::Instantiation(format!("fuel: {e}")))?;

        // 2. Link builtins + shader
        // This mirrors wasm_link.rs logic
        let (instance, memory) = self.link_and_instantiate(&mut store)?;

        // 3. Create WasmInstance
        Ok(WasmInstance::new(
            store,
            instance,
            memory,
            self.wasm_module.shadow_stack_base(),
        ))
    }
}

const DEFAULT_FUEL: u64 = 1_000_000;
```

**Linking implementation** (adapt from `wasm_link.rs`):

```rust
impl WasmRuntimeModule {
    fn link_and_instantiate(
        &self,
        store: &mut Store<()>,
    ) -> Result<(Instance, Option<Memory>), WasmError> {
        // TODO: Parse builtins module, link memory, link builtin functions
        // Adapt from lps-filetests/src/test_run/wasm_link.rs
        // Simplified: just instantiate shader if no imports needed
        // Full version: link with lps-builtins-wasm.wasm
        todo!("link_and_instantiate")
    }
}
```

**runtime/instance.rs - full implementation:**

```rust
use alloc::vec::Vec;
use lps_shared::lps_value::LpsValue;
use lpvm::LpvmInstance;
use wasmtime::{Instance, Memory, Store, Val};
use crate::error::WasmError;
use crate::module::WasmExport;

pub struct WasmInstance {
    store: Store<()>,
    instance: Instance,
    memory: Option<Memory>,
    shadow_stack_base: Option<i32>,
    // TODO: cache exports lookup
}

impl WasmInstance {
    pub(crate) fn new(
        store: Store<()>,
        instance: Instance,
        memory: Option<Memory>,
        shadow_stack_base: Option<i32>,
    ) -> Self {
        Self {
            store,
            instance,
            memory,
            shadow_stack_base,
        }
    }

    fn prepare_call(&mut self) -> Result<(), WasmError> {
        // Reset shadow stack if present
        if let Some(base) = self.shadow_stack_base {
            let global = self.instance
                .get_global(&mut self.store, "__lp_shadow_sp")
                .ok_or_else(|| WasmError::Call("missing shadow stack global".into()))?;
            global.set(&mut self.store, Val::I32(base))
                .map_err(|e| WasmError::Call(format!("shadow stack reset: {e}")))?;
        }

        // Reset fuel
        self.store.set_fuel(DEFAULT_FUEL)
            .map_err(|e| WasmError::Call(format!("fuel reset: {e}")))?;

        Ok(())
    }
}

impl LpvmInstance for WasmInstance {
    type Error = WasmError;

    fn call(
        &mut self,
        name: &str,
        args: &[LpsValue],
    ) -> Result<LpsValue, Self::Error> {
        // 1. Prepare call (shadow stack, fuel)
        self.prepare_call()?;

        // 2. Find export and get function
        let func = self.instance
            .get_func(&mut self.store, name)
            .ok_or_else(|| WasmError::Call(format!("function '{name}' not found")))?;

        // 3. Marshal args (LpsValue -> WASM Val)
        // TODO: get export metadata for type conversion
        let wasm_args = marshal_args(args)?;

        // 4. Call
        let mut results = vec![Val::I32(0); /* result count */];
        func.call(&mut self.store, &wasm_args, &mut results)
            .map_err(|e| WasmError::Call(format!("trap: {e}")))?;

        // 5. Unmarshal result (WASM Val -> LpsValue)
        // TODO: lookup return type from export metadata
        unmarshal_result(&results)
    }
}

// TODO: marshal_args, unmarshal_result functions
// These need export metadata to know types
```

### Design Notes

- **Export metadata needed:** `WasmInstance` needs to know the `WasmExport`
  info for type marshaling. Should store `Vec<WasmExport>` or a map.

- **Function signature:** The current `call()` API doesn't tell us the expected
  return type. We need to look it up from the export metadata.

- **Argument flattening:** `vec3` becomes 3 `I32` or `F32` values depending on
  float mode. The marshaling functions need the `WasmExport` info.

### Simplification for Phase 6

For initial implementation:
1. Store `Vec<WasmExport>` in `WasmInstance`
2. `marshal_args`: look up export by name, use `param_types` to flatten
3. `unmarshal_result`: use `return_type` from export

Full Q32 marshaling (value * 65536.0) can be stubbed with TODO — just pass
values through as I32 for now.

### Validate

```bash
cargo check -p lpvm-wasm --features runtime 2>&1 | head -40
```

Fix compilation errors. Many TODO bodies are expected.
