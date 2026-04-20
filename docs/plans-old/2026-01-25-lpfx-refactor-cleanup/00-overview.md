# Plan: LPFX Refactor Cleanup

## Overview

Clean up and fix all code that does direct string checks against LP library function names, replacing them with proper use of `LpfnFnId` as the single source of truth. This ensures the codebase is maintainable and correctly handles the recent reorganization of builtin functions.

## Phases

1. Extend `LpfnFnId` with missing information methods
2. Update generator to use `LpfnFnId` instead of string checks
3. Fix testcase mapping generation
4. Update `is_lp_lib_fn` to check correct prefix
5. Verify and test all changes
6. Cleanup and finalization

## Success Criteria

- All direct string checks against function names are replaced with `LpfnFnId` method calls
- Generator code uses `LpfnFnId` as single source of truth
- Testcase mapping correctly uses `LpfnFnId` information
- All tests pass
- Code compiles without warnings
- Code formatted with `cargo +nightly fmt`
