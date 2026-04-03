# Phase 3: Update Cranelift Signatures

## Scope of Phase

Update `signature_for_ir_func` and related code in `lpir-cranelift` to include VMContext as the first parameter in all function signatures.

## Code Organization Reminders

- Update `signature_for_ir_func` to prepend VMContext param
- Update param indexing in `emit/mod.rs` to account for VMContext
- Keep related signature logic grouped together

## Implementation Details

### 1. Update `lpir-cranelift/src/emit/mod.rs`

Modify `signature_for_ir_func`:

```rust
pub fn signature_for_ir_func(
    func: &IrFunction,
    call_conv: CallConv,
    mode: FloatMode,
    pointer_type: types::Type,
    isa: &dyn TargetIsa,
) -> Signature {
    let mut sig = Signature::new(call_conv);
    let sr = signature_uses_struct_return(isa, func);
    
    // VMContext always first (before even StructReturn pointer)
    sig.params.push(AbiParam::new(pointer_type));
    
    if sr {
        sig.params.push(AbiParam::special(
            pointer_type,
            ArgumentPurpose::StructReturn,
        ));
    }
    
    // User params follow VMContext
    for i in 0..func.param_count as usize {
        let vreg_idx = func.vmctx_vreg.0 as usize + 1 + i;
        sig.params.push(AbiParam::new(ir_type_for_mode(
            func.vreg_types[vreg_idx],
            mode
        )));
    }
    
    if !sr {
        for t in &func.return_types {
            sig.returns.push(AbiParam::new(ir_type_for_mode(*t, mode)));
        }
    }
    sig
}
```

Update `translate_function` param handling:

```rust
pub fn translate_function(
    func: &IrFunction,
    builder: &mut FunctionBuilder,
    ctx: &EmitCtx,
) -> Result<(), CompileError> {
    let mut vars = Vec::with_capacity(func.vreg_types.len());
    for ty in &func.vreg_types {
        vars.push(builder.declare_var(ir_type_for_mode(*ty, ctx.float_mode)));
    }

    let entry = builder.current_block().expect("entry block");
    let params: Vec<Value> = builder.block_params(entry).to_vec();
    
    // params[0] is VMContext
    let vmctx_val = params[0];
    def_v(builder, &vars, func.vmctx_vreg, vmctx_val);
    
    // User params follow
    let param_base = usize::from(ctx.uses_struct_return) + 1;  // +1 for vmctx
    for (i, val) in params.iter().enumerate().skip(param_base) {
        let user_idx = i - param_base;
        if user_idx < func.param_count as usize {
            def_v(builder, &vars, func.user_param_vreg(user_idx as u16), *val);
        }
    }
    
    // ... rest of function
}
```

### 2. Update call emission

In `lpir-cranelift/src/emit/call.rs`, update to pass VMContext:

```rust
// When emitting a call, include VMContext as first arg
let vmctx_val = use_v(builder, vars, func.vmctx_vreg);
let mut call_args = vec![vmctx_val];
// ... add user args
call_args.push(use_v(builder, vars, arg_vreg));

builder.ins().call(func_ref, &call_args);
```

## Tests to Write

Verify signatures include VMContext:

```rust
#[test]
fn signature_has_vmctx_first() {
    let func = IrFunction::new(2);  // 2 user params
    let sig = signature_for_ir_func(&func, ...);
    
    // VMContext + 2 user params
    assert_eq!(sig.params.len(), 3);
    
    // First param is pointer type
    assert_eq!(sig.params[0].value_type, pointer_type);
}
```

## Validate

```bash
cargo test -p lpir-cranelift
cargo check -p lpir-cranelift --target riscv32imac-unknown-none-elf
```

## Notes

- StructReturn pointer (if present) comes AFTER VMContext
- This maintains ABI consistency: vmctx is always param 0, sret is param 1 (if present)
