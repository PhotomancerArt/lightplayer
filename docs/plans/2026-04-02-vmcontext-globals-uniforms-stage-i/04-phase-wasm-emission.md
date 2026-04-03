# Phase 4: Update WASM Emission

## Scope of Phase

Update `lp-glsl-wasm` to include VMContext as the first local variable (`local.get 0`) in all shader functions.

## Code Organization Reminders

- Add `vmctx_local` to `FuncEmitCtx` near other local tracking fields
- Update `emit_module` to initialize `vmctx_local` to `Some(0)`
- Update all local indexing to account for VMContext

## Implementation Details

### 1. Update `lp-glsl-wasm/src/emit/mod.rs`

Add `vmctx_local` to `FuncEmitCtx`:

```rust
pub(crate) struct FuncEmitCtx<'a> {
    pub module: &'a EmitCtx<'a>,
    pub vmctx_local: Option<u32>,  // NEW: local index for VMContext, always Some(0)
    pub i64_scratch: Option<u32>,
    pub sp_global: Option<u32>,
    pub frame_size: u32,
    pub slot_offsets: &'a [u32],
    pub result_buffer_base_offset: u32,
    pub unreachable_mode: bool,
}
```

Update `emit_module` to set `vmctx_local`:

```rust
let sp_global = if needs_shadow_stack { Some(0u32) } else { None };
// Note: WASM globals and function locals are separate namespaces
// vmctx is the first local in each function, not a global

for f in &ir.functions {
    let ctx = FuncEmitCtx {
        module: &ctx,
        vmctx_local: Some(0),  // NEW: local 0 is always VMContext
        i64_scratch: None,     // Will be calculated
        sp_global,
        frame_size: calc_frame_size(f),
        slot_offsets: &slot_offsets,
        result_buffer_base_offset,
        unreachable_mode: false,
    };
    let wasm_fn = func::encode_ir_function(ir, f, &ctx, sp_global)?;
    code.function(&wasm_fn);
}
```

### 2. Update `lp-glsl-wasm/src/emit/func.rs`

Update local indexing. User locals now start at index 1 (after VMContext at index 0):

```rust
// In function prologue, locals are:
// 0: vmctx (i32) - NEW
// 1..N: user params (from function signature)
// N..M: locals declared in function

pub fn encode_ir_function(
    ir: &IrModule,
    func: &IrFunction,
    ctx: &FuncEmitCtx,
    sp_global: Option<u32>,
) -> Result<Function, String> {
    let vmctx_local = ctx.vmctx_local.expect("vmctx always present");
    
    // Calculate local indices
    // local 0: vmctx
    // local 1..func.param_count+1: user params
    // local func.param_count+1..: declared locals
    
    let user_param_base = 1u32;  // After vmctx
    let declared_local_base = 1u32 + func.param_count as u32;
    
    // ...
}
```

Update param handling in function body:

```rust
// OLD: user param i is at local index i
// NEW: user param i is at local index i + 1 (after vmctx)

for i in 0..func.param_count {
    let local_idx = user_param_base + i as u32;
    // Map user param i to local index
}
```

### 3. Update import handling

When calling imported functions (builtins), we need to pass VMContext as well:

```rust
// Before calling a builtin, push vmctx first
if ctx.vmctx_local.is_some() {
    builder.local_get(vmctx_local);
}
// Then push other args
```

Wait—builtins don't need VMContext. Only shader functions do. So builtins keep their current signatures, shader functions add VMContext.

Actually, for consistency, let's pass VMContext to builtins too (they can ignore it). This keeps the ABI uniform.

## Tests to Write

```rust
#[test]
fn wasm_function_has_vmctx_local() {
    let ir = create_simple_ir();
    let (wasm, _) = emit_module(&ir, &WasmOptions::default()).unwrap();
    
    // Decode WASM and verify first local is i32 (vmctx pointer)
    // This is a basic smoke test
}
```

## Validate

```bash
cargo test -p lp-glsl-wasm
cargo check -p lp-glsl-wasm --target riscv32imac-unknown-none-elf
```

## Notes

- VMContext is a function local (not a global) so each shader function gets it as param 0
- Builtins may or may not receive VMContext—we need to decide. Simpler if all functions get it.
