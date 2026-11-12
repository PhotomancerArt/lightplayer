# Phase 2: Add CraneliftEngine (LpvmEngine implementation)

## Goal

Create `CraneliftEngine` that implements the `LpvmEngine` trait. This is the
entry point for compiling LPIR to a module using Cranelift JIT.

## Design

```rust
pub struct CraneliftEngine {
    options: CompileOptions,
}

impl CraneliftEngine {
    pub fn new(options: CompileOptions) -> Self {
        Self { options }
    }
}

impl LpvmEngine for CraneliftEngine {
    type Module = CraneliftModule;
    type Error = CompileError;
    
    fn compile(&self, ir: &lpir::IrModule, meta: &ShaderMetadata) -> Result<Self::Module, Self::Error> {
        // 1. Configure ISA (native or riscv32)
        // 2. Build JITModule
        // 3. Lower LPIR functions to Cranelift IR
        // 4. Finalize definitions
        // 5. Extract code pointers and metadata
        // 6. Return CraneliftModule
    }
}
```

## Implementation Notes

Reuse existing code from `compile.rs` and `jit_module.rs`:
- `build_jit_module()` for JIT setup
- `lower_ir_to_clif()` for lowering
- ISA selection: `cranelift-native` on host, hardcoded `riscv32imac` on embedded

## Files to Create/Modify

- `lp-shader/lpvm-cranelift/src/engine.rs` (NEW)
- `lp-shader/lpvm-cranelift/src/lib.rs` (add module + re-export)

## Tests

```rust
#[test]
fn test_compile_simple_add() {
    let engine = CraneliftEngine::new(CompileOptions::default());
    let ir = simple_add_ir();  // I32 add function
    let meta = simple_metadata();
    
    let module = engine.compile(&ir, &meta).unwrap();
    // Module should have the function
    assert!(module.has_function("add"));
}
```

## Done When

- `CraneliftEngine` implements `LpvmEngine`
- Can compile simple LPIR to module
- Unit test passes
- Host and RISC-V targets compile
