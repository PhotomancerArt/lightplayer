# Phase 4: Add Extern Declarations for no_std Host Functions

## Description

Add extern function declarations for `lp_jit_debug` and `lp_jit_print` that users must provide in no_std mode. These will be resolved by the linker.

## Implementation

1. In `host/mod.rs`, add extern block for no_std:
   ```rust
   #[cfg(not(feature = "std"))]
   extern "C" {
       fn lp_jit_debug(ptr: *const u8, len: usize);
       fn lp_jit_print(ptr: *const u8, len: usize);
   }
   ```
2. Export these so they can be referenced from `registry.rs`

## Success Criteria

- Extern declarations compile in no_std mode
- Functions are accessible from `registry.rs`
- Documentation notes that users must provide these implementations
