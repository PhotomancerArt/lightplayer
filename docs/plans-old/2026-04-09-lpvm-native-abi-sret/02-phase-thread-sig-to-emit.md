## Scope of Phase

Thread `LpsFnSig` through emission pipeline so `emit_function_bytes` can access ABI info.

## Implementation Details

### 1. Update `emit.rs` function signature

```rust
/// Emit one function to RV32 bytes (and relocations). 
/// Now takes LpsFnSig for ABI classification.
pub fn emit_function_bytes(
    func: &lpir::IrFunction,
    sig: &LpsFnSig,  // NEW: for sret detection
    float_mode: lpir::FloatMode,
    debug_info: bool,
) -> Result<EmittedFunction, NativeError> {
    let vinsts = crate::lower::lower_ops(func, float_mode)?;
    let alloc = RegAlloc::allocate(&GreedyAlloc, func, &vinsts)?;
    
    // NEW: Get ABI info for sret handling
    let abi_info = abi::AbiInfo::from_ir_and_sig(func, sig);
    
    let is_leaf = !vinsts.iter().any(|v| v.is_call());
    let frame = if is_leaf && alloc.spill_count() == 0 {
        abi::leaf_frame()
    } else {
        abi::nonleaf_frame(alloc.spill_count())
    };
    
    let mut ctx = EmitContext::with_frame(frame, debug_info);
    ctx.emit_prologue();
    for v in &vinsts {
        ctx.emit_vinst(v, &alloc, &abi_info)?;  // NEW: pass abi_info
    }
    ctx.emit_epilogue(&abi_info);  // NEW: pass abi_info for sret returns
    
    Ok(EmittedFunction {
        code: ctx.code,
        relocs: ctx.relocs,
        debug_lines: ctx.debug_lines,
    })
}
```

### 2. Update `emit_module_elf`

```rust
pub fn emit_module_elf(
    ir: &lpir::IrModule,
    meta: &LpsModuleSig,  // NEW: needed for per-function signatures
    float_mode: lpir::FloatMode,
) -> Result<Vec<u8>, NativeError> {
    // ... existing setup ...
    
    for func in &ir.functions {
        // NEW: Lookup signature for this function
        let sig = meta.functions.iter()
            .find(|f| f.name == func.name)
            .ok_or_else(|| NativeError::Internal(format!("no sig for {}", func.name)))?;
        
        let emitted = emit_function_bytes(func, sig, float_mode, false)?;
        // ... rest unchanged ...
    }
    // ...
}
```

### 3. Update `debug_asm.rs`

```rust
pub fn compile_module_asm_text(
    ir: &IrModule,
    meta: &LpsModuleSig,  // NEW
    float_mode: lpir::FloatMode,
    opts: DisasmOptions,
) -> Result<String, NativeError> {
    // ... lookup sig per function like emit_module_elf ...
}
```

### 4. Update `NativeEmuEngine::compile`

```rust
fn compile(&self, ir: &IrModule, meta: &LpsModuleSig) -> Result<Self::Module, Self::Error> {
    let elf = emit_module_elf(ir, meta, self.options.float_mode)?;  // NEW: pass meta
    // ...
}
```

## Tests to Write

No new tests - compilation will verify the signature threading works.

## Validate

```bash
cargo check -p lpvm-native
cargo test -p lpvm-native --lib
```

**Note:** This will break temporarily since emit_vinst doesn't use abi_info yet - that's phase 3.
