## Phase 7: Runtime Tests

### Scope

Add integration tests for the full LPVM trait pipeline:
- Compile LPIR → WASM
- Instantiate with wasmtime
- Call functions with `LpsValue` arguments
- Verify results and side effects (shadow stack, fuel)

### Implementation Details

**tests/runtime_integration.rs:**

```rust
//! Integration tests for LPVM WASM runtime.
//
// These tests require the `runtime` feature and valid builtins WASM.

use lpir::parse_module;
use lps_shared::LpsModuleSig;
use lpvm::{LpvmEngine, LpvmInstance, LpvmModule};
use lpvm_wasm::runtime::WasmEngine;
use lpvm_wasm::WasmOptions;

fn load_builtins_bytes() -> Vec<u8> {
    // TODO: Load from target/wasm32-unknown-unknown/release/lps_builtins_wasm.wasm
    // For now, return empty (tests that need builtins will fail with clear error)
    Vec::new()
}

#[test]
#[cfg(feature = "runtime")]
fn compile_simple_function() {
    let ir = parse_module(r#"
func @identity(v1:i32) -> i32 {
  return v1
}
"#).expect("parse");

    let meta = LpsModuleSig {
        functions: vec![/* identity signature */],
    };

    let builtins = load_builtins_bytes();
    let engine = WasmEngine::new(builtins).expect("create engine");
    let module = engine.compile(&ir, &meta).expect("compile");

    // Verify we can access raw bytes
    assert!(!module.emission().bytes().is_empty());

    // Verify signatures
    assert_eq!(module.signatures().functions.len(), 1);
}

#[test]
#[cfg(feature = "runtime")]
fn instantiate_and_call_i32() {
    let ir = parse_module(r#"
func @add(v1:i32, v2:i32) -> i32 {
  v3:i32 = iadd v1, v2
  return v3
}
"#).expect("parse");

    let meta = LpsModuleSig { /* add signature */ };

    let builtins = load_builtins_bytes();
    let engine = WasmEngine::new(builtins).expect("create engine");
    let module = engine.compile(&ir, &meta).expect("compile");
    let mut instance = module.instantiate().expect("instantiate");

    // Call with LpsValue
    use lps_shared::LpsValue;
    let result = instance.call("add", &[LpsValue::I32(2), LpsValue::I32(3)])
        .expect("call");

    assert_eq!(result, LpsValue::I32(5));
}

#[test]
#[cfg(feature = "runtime")]
fn call_fuel_consumption() {
    // Test that fuel is consumed and limits work
    let ir = parse_module(r#"
func @loop_forever() {
  loop {
    br_if_not 0  // never breaks
  }
}
"#).expect("parse");

    // TODO: Test with low fuel limit, verify trap on fuel exhaustion
}

#[test]
#[cfg(feature = "runtime")]
fn shadow_stack_reset() {
    // Test that shadow stack is reset between calls
    let ir = parse_module(r#"
func @uses_stack() {
  slot s0, 4
  // ...
}
"#).expect("parse");

    // TODO: Multiple calls, verify stack doesn't overflow incorrectly
}
```

### Test Data Setup

For tests to pass:
1. **No-builtin tests:** Simple functions (iadd, iconst) don't need builtins
2. **With-builtin tests:** Need `lps-builtins-wasm` compiled:
   ```bash
   cargo build -p lps-builtins-wasm --target wasm32-unknown-unknown --release
   ```

Load builtins bytes from:
`target/wasm32-unknown-unknown/release/lps_builtins_wasm.wasm`

### Error Handling

If builtins not available:
- Skip tests with `#[ignore = "requires builtins wasm"]`
- Or check at runtime and return early with clear message

### Validate

```bash
# Build builtins first
just build-builtins  # or manual cargo command

# Run tests
cargo test -p lpvm-wasm --features runtime -- --test-threads=1
```

Expected: Some tests pass (no-builtin), some may fail (with-builtin) if
marshaling is not fully implemented. That's OK for this phase — we verify
the pipeline works end-to-end.
