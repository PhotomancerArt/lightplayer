# Plan: LPFX Registry Refactor

## Overview

Refactor the LPFX function system to use a registry-based approach that eliminates hardcoded checks and makes adding new functions trivial. The system will be fully dynamic, driven by function signatures stored in the registry.

## Phases

1. Update core data structures
2. Define function registry in lpfx_fns.rs
3. Implement registry lookup functions
4. Implement signature conversion helpers
5. Update semantic checking to use registry
6. Update codegen to use registry
7. Update backend transforms to use registry
8. Update builtin registry integration
9. Update lp-builtin-gen to use registry
10. Remove LpfxFnId enum and cleanup
11. Final cleanup and testing

## Success Criteria

- All hardcoded function name checks removed
- Adding a new function requires only adding an entry to `lpfx_fns.rs`
- No match statements on specific function names
- All type conversion and signature handling is dynamic
- Code compiles and tests pass
- Code formatted with `cargo +nightly fmt`
