# Phase 5: Update DirectCall and Invoke APIs

## Scope of Phase

Update `DirectCall::call_i32()` and related APIs in `lpvm-cranelift` to accept VMContext as a
separate parameter. Update the `invoke` module to prepend VMContext to args.

## Code Organization Reminders

- Update `DirectCall` methods to take `vmctx: *const u8` as first param
- Update `invoke::invoke_i32_args_returns` similarly
- Place helper functions at the bottom of files

## Implementation Details

### 1. Update `lpvm-cranelift/src/lib.rs`

Modify `DirectCall::call_i32()`:

```rust
impl DirectCall {
    /// Call the JIT-compiled function with VMContext and args.
    ///
    /// # Safety
    /// VMContext pointer must be valid for the duration of the call.
    pub unsafe fn call_i32(
        &self,
        vmctx: *const u8,        // NEW: VMContext pointer
        args: &[i32],
    ) -> Result<Vec<i32>, String> {
        // Prepend vmctx to args for invoke
        let mut full_args = Vec::with_capacity(1 + args.len());
        full_args.push(vmctx as i32);  // On 32-bit targets, pointer fits in i32
        full_args.extend_from_slice(args);
        
        unsafe {
            invoke::invoke_i32_args_returns(
                self.code,
                &full_args,
                self.n_ret,
                self.uses_struct_return,
            )
        }
    }

    /// Call with buffer output (no heap allocation for returns).
    pub unsafe fn call_i32_buf(
        &self,
        vmctx: *const u8,        // NEW
        args: &[i32],
        out: &mut [i32],
    ) -> Result<(), String> {
        let mut full_args = Vec::with_capacity(1 + args.len());
        full_args.push(vmctx as i32);
        full_args.extend_from_slice(args);
        
        unsafe {
            invoke::invoke_i32_args_returns_buf(
                self.code,
                &full_args,
                out.len(),
                out,
                self.uses_struct_return,
            )
        }
    }
}
```

### 2. Update `lpvm-cranelift/src/invoke.rs`

The invoke functions already receive args as a slice, so they don't need changes to their
signatures. They just need to handle the fact that args[0] is now VMContext.

However, we should document this:

```rust
/// Invoke JIT code with i32 args and returns.
/// 
/// # Arguments
/// * `code` - Pointer to JIT-compiled function
/// * `args` - Arguments to pass. args[0] is VMContext pointer, args[1..] are user args.
/// * `n_ret` - Expected number of return values
/// * `uses_struct_return` - Whether the function uses struct return ABI
pub unsafe fn invoke_i32_args_returns(
    code: *const u8,
    args: &[i32],
    n_ret: usize,
    uses_struct_return: bool,
) -> Result<Vec<i32>, String> {
    // ... existing implementation
    // Note: args[0] is VMContext, handled same as other args
}
```

### 3. Update tests in `lpvm-cranelift/src/lib.rs`

Update existing tests to pass VMContext:

```rust
#[test]
fn test_direct_call() {
    let module = compile_test_shader();
    let vmctx = minimal_vmcontext();
    
    let dc = module.direct_call("test_func").unwrap();
    let result = unsafe {
        dc.call_i32(vmctx.as_ptr(), &[1, 2]).unwrap()
    };
    
    assert_eq!(result, vec![3]);
}
```

### 4. Update `emu_run.rs` if needed

Check if `emu_run.rs` calls shaders directly and update accordingly.

## Tests to Write

```rust
#[test]
fn direct_call_accepts_vmctx() {
    // Test that DirectCall::call_i32 accepts vmctx param
    // and passes it through to the shader
}

#[test]
fn invoke_receives_vmctx_in_args() {
    // Test that invoke functions correctly handle vmctx in args[0]
}
```

## Validate

```bash
cargo test -p lpvm-cranelift
cargo check -p lpvm-cranelift --target riscv32imac-unknown-none-elf
```

## Notes

- On 64-bit targets, we'd need to handle pointer-to-i32 conversion differently
- For now we assume 32-bit (RISC-V32, WASM32) where pointers fit in i32
