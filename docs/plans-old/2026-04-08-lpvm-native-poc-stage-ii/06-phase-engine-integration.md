# Phase 6: Engine Integration

## Scope

Connect `NativeEngine::compile()` through the full pipeline: LPIR → lowering → regalloc → emission → ELF. Update `NativeModule` to store ELF bytes.

## Code Organization

- Update `engine.rs` compile method
- Update `module.rs` NativeModule struct
- Integration test

## Implementation Details

Update `module.rs`:
```rust
/// Compiled native module (ELF object file).
#[derive(Debug, Clone)]
pub struct NativeModule {
    /// ELF object file bytes
    pub elf: Vec<u8>,
    /// Function name (for debugging/symbols)
    pub name: String,
    /// Size of code section
    pub code_size: usize,
}
```

Update `engine.rs`:
```rust
use crate::lower;
use crate::regalloc::GreedyAlloc;
use crate::isa::rv32::emit::EmitContext;

impl LpvmEngine for NativeEngine {
    type Module = NativeModule;
    type Instance = NativeInstance;
    type Memory = BumpLpvmMemory;
    type CompileOptions = NativeCompileOptions;
    
    fn compile(
        &self,
        ir: &IrFunction,
        opts: &NativeCompileOptions,
    ) -> Result<NativeModule, NativeError> {
        // 1. Lower to VInst
        let vinsts = lower::lower_ops(&ir.body)?;
        
        // 2. Register allocation
        let alloc = self.reg_alloc.allocate(ir)?;
        
        // 3. Determine if leaf (no calls in VInsts)
        let is_leaf = !vinsts.iter().any(|v| matches!(v, VInst::Call { .. }));
        
        // 4. Emit code
        let mut ctx = EmitContext::new(is_leaf);
        ctx.emit_prologue();
        
        for (i, vinst) in vinsts.iter().enumerate() {
            // Get allocation for this instruction
            let op_alloc = &alloc.per_op[i];
            ctx.emit_vinst(vinst, op_alloc);
        }
        
        ctx.emit_epilogue();
        
        // 5. Generate ELF
        let code_size = ctx.code.len();
        let elf = ctx.finish_elf(&ir.name)?;
        
        Ok(NativeModule {
            elf,
            name: ir.name.clone(),
            code_size,
        })
    }
    
    fn memory(&self) -> &Self::Memory {
        &self.memory
    }
}
```

## Lowering Update

`lower::lower_ops` needs to return `Result<Vec<VInst>, NativeError>`:
```rust
pub fn lower_ops(ops: &[Op]) -> Result<Vec<VInst>, NativeError> {
    let mut vinsts = Vec::new();
    for op in ops {
        let v = lower_op(op)?;
        vinsts.push(v);
    }
    Ok(vinsts)
}
```

## Integration Test

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::NativeEngine;
    use crate::module::NativeModule;
    use lpir::{IrFunction, Op, VReg, Slot};
    use alloc::vec;
    
    fn build_simple_add() -> IrFunction {
        IrFunction {
            name: "simple_add".into(),
            vreg_types: vec![lpir::IrType::I32; 3], // v0, v1, v2
            slots: vec![],
            body: vec![
                // v2 = v0 + v1
                Op::Add { dst: 2, src1: 0, src2: 1 },
                // return v2
                Op::Return { val: Some(2) },
            ],
        }
    }
    
    #[test]
    fn test_compile_pipeline() -> Result<(), NativeError> {
        let engine = NativeEngine::new();
        let func = build_simple_add();
        let opts = NativeCompileOptions::default();
        
        let module = engine.compile(&func, &opts)?;
        
        // Verify ELF was generated
        assert!(module.elf.len() > 0);
        assert_eq!(module.elf[0..4], [0x7f, 0x45, 0x4c, 0x46]); // ELF magic
        assert_eq!(module.name, "simple_add");
        
        // Verify ELF structure
        let obj = object::File::parse(&*module.elf)
            .map_err(|e| NativeError::EmitError(e.to_string()))?;
        assert_eq!(obj.architecture(), object::Architecture::Riscv32);
        
        Ok(())
    }
    
    #[test]
    fn test_compile_q32_builtin() -> Result<(), NativeError> {
        let engine = NativeEngine::new();
        let func = IrFunction {
            name: "q32_add".into(),
            vreg_types: vec![lpir::IrType::F32; 3], // Q32 treated as F32 in LPIR
            slots: vec![],
            body: vec![
                // v2 = fadd(v0, v1) -> lowers to Call __lpir_fadd_q32
                Op::FAdd { dst: 2, src1: 0, src2: 1 },
                Op::Return { val: Some(2) },
            ],
        };
        
        let module = engine.compile(&func, &NativeCompileOptions::default())?;
        
        // Verify ELF has relocation
        let obj = object::File::parse(&*module.elf)
            .map_err(|e| NativeError::EmitError(e.to_string()))?;
        
        // Check for relocation entries
        let has_relocs = obj.sections()
            .any(|s| s.kind() == object::SectionKind::LinkRelocation);
        
        assert!(has_relocs, "ELF should have relocation section for builtin call");
        
        Ok(())
    }
}
```

## Key Points

- Full pipeline: LPIR ops → VInsts → Allocation → Machine code → ELF
- `is_leaf` detection determines prologue complexity
- ELF bytes stored in `NativeModule.elf`

## Validate

```bash
cargo test -p lpvm-native --lib engine::tests
cargo check -p lpvm-native
```
