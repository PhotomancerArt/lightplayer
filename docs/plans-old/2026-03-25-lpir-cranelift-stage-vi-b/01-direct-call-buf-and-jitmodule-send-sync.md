# Phase 1: `DirectCall::call_i32_buf` + `JitModule` Send/Sync

## Scope

Add a non-allocating invoke path for Level-3 calls and mark `JitModule` as
`Send + Sync` so `ShaderRuntime` can store it without a trait object.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### 1. `invoke.rs` — `invoke_i32_args_returns_buf`

Add a sibling to `invoke_i32_args_returns` that writes return scalars into
`out: &mut [i32]` instead of allocating a `Vec`.

- Reuse the same arity / `n_ret` validation as `invoke_i32_args_returns`.
- For `n_ret == 0` / `1`: write nothing or `out[0]` as appropriate.
- For `n_ret` in `2..=4`: on non-Apple-AArch64, call existing `invoke_cretN`
  helpers and copy `r.v0, …` into `out[0..n_ret]`.
- For Apple AArch64: extend `aarch64_invoke_multi_ret` with a variant that
  writes into `out` instead of `alloc::vec![…]`, or factor shared logic to avoid
  duplicating every `asm!` block.

Document `# Safety` the same as `invoke_i32_args_returns`.

### 2. `direct_call.rs` — `call_i32_buf`

```rust
impl DirectCall {
    /// Like [`Self::call_i32`] but writes returns into `out` (no heap allocation).
    ///
    /// # Safety
    /// Same as [`Self::call_i32`].
    pub unsafe fn call_i32_buf(
        &self,
        args: &[i32],
        out: &mut [i32],
    ) -> Result<(), CallError> {
        if out.len() != self.ret_i32_count {
            return Err(CallError::Arity { ... }); // or a dedicated variant
        }
        unsafe {
            crate::invoke::invoke_i32_args_returns_buf(
                self.func_ptr,
                args,
                self.ret_i32_count,
                out,
            )
        }
    }
}
```

Keep existing `call_i32` delegating to `invoke_i32_args_returns` for callers
that prefer a `Vec`.

### 3. `jit_module.rs` — `Send` / `Sync`

After `pub struct JitModule { … }`, add:

```rust
// SAFETY: Finalized JIT code is immutable. `JITModule` is not mutated after
// `build_jit_module` returns. The engine holds the module on a single thread
// today; `NodeRuntime: Send + Sync` requires these impls for boxed runtimes.
unsafe impl Send for JitModule {}
unsafe impl Sync for JitModule {}
```

### 4. Tests

- Unit test: `call_i32_buf` with a tiny IR module (e.g. F32 add or Q32 const)
  matching a known arity, compare results to `call_i32`.
- Optional: `#[cfg(test)]` only, no new file if tests fit in `lib.rs` or
  `direct_call.rs` test module.

## Validate

```bash
cargo test -p lpvm-cranelift
cargo test -p lpvm-cranelift --features riscv32-emu
cargo clippy -p lpvm-cranelift --all-features -- -D warnings
```
