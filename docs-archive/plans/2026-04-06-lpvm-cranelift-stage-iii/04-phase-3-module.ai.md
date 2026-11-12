# Phase 3: Add CraneliftModule (LpvmModule implementation)

## Goal

Create `CraneliftModule` that implements `LpvmModule`. This represents
finalized JIT code that can be instantiated multiple times.

## Design

```rust
pub struct CraneliftModule {
    // Code pointers extracted from JITModule
    code_ptrs: HashMap<String, *const u8>,
    // Function signatures for call marshaling
    signatures: HashMap<String, FuncSignature>,
    // Metadata for functions
    metadata: ShaderMetadata,
}

impl LpvmModule for CraneliftModule {
    type Instance = CraneliftInstance;
    type Error = CallError;
    
    fn instantiate(&self) -> Result<Self::Instance, Self::Error> {
        // Create new instance with fresh memory
        CraneliftInstance::new(self)
    }
}

impl CraneliftModule {
    // Beyond trait: hot path direct call
    pub fn direct_call(&self, name: &str) -> Option<DirectCall> {
        // Return DirectCall with code pointer and signature
        let ptr = self.code_ptrs.get(name)?;
        let sig = self.signatures.get(name)?;
        Some(DirectCall::new(*ptr, sig.clone()))
    }
}
```

## Implementation Notes

- Extract code pointers during finalization
- Store signatures from LPIR
- `DirectCall` is the existing type from old API
- Module is immutable after creation (no JITModule held)

## Files to Create/Modify

- `lp-shader/lpvm-cranelift/src/module.rs` (NEW)
- `lp-shader/lpvm-cranelift/src/lib.rs` (add module)

## Integration with Old API

The existing `JitModule` stays. `CraneliftModule` is the new trait-based
version. They may share code pointer extraction logic.

## Tests

```rust
#[test]
fn test_module_has_function() {
    let engine = CraneliftEngine::new(CompileOptions::default());
    let module = engine.compile(&ir, &meta).unwrap();
    
    assert!(module.has_function("render"));
    assert!(module.direct_call("render").is_some());
}

#[test]
fn test_module_instantiate() {
    let module = compile_test_module();
    let inst1 = module.instantiate().unwrap();
    let inst2 = module.instantiate().unwrap();
    // Two independent instances
}
```

## Done When

- `CraneliftModule` implements `LpvmModule`
- `direct_call()` method available
- Can instantiate multiple instances
- Unit tests pass
