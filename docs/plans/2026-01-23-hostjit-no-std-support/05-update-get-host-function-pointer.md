# Phase 5: Update `get_host_function_pointer()` for no_std Support

## Description

Modify `get_host_function_pointer()` to return pointers to extern functions in no_std mode instead of returning `None`.

## Implementation

1. Update `get_host_function_pointer()` in `host/registry.rs`
2. In no_std mode: return `Some(lp_jit_debug as *const u8)` or `Some(lp_jit_print as *const u8)`
3. Map `HostId::Debug` → `lp_jit_debug` and `HostId::Println` → `lp_jit_print`
4. Keep std mode behavior unchanged (returns pointers to `impls.rs` functions)

## Success Criteria

- Function returns correct pointers in no_std mode
- Function returns correct pointers in std mode (unchanged)
- Symbol lookup works correctly in both modes
