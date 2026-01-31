# Plan: Make Q32 Transform Use Builtins for Add, Sub, and Div

## Overview

Update the q32 transform to use builtin functions for `add`, `sub`, and `div` operations instead of
generating inline saturation code. This will reduce code bloat from ~20-30 instructions per
operation to a single function call, following the same pattern already established for `mul`.

## Phases

1. Implement add and sub builtins
2. Verify div builtin edge cases
3. Update transform to use builtins
4. Regenerate builtin registry
5. Update tests and verify correctness
6. Verify code size reduction
7. Cleanup and finalization

## Success Criteria

- `__lp_q32_add` and `__lp_q32_sub` builtins implemented and working
- `convert_fadd`, `convert_fsub`, `convert_fdiv` use builtins instead of inline code
- All tests pass (including unignored `test_q32_fdiv`)
- Code size reduction verified via lp-glsl-q32-metrics-app comparison
- Code formatted with `cargo +nightly fmt`
- All warnings fixed
