# Plan: Fix GLSL Test Failures

## Overview

Fix 25 failing tests in the GLSL compiler test suite. Tests fall into 4 categories:
1. JIT tests using unsupported Float format (4 tests) - Simple fix
2. Emulator basic execution returning 0 (3 tests) - Core bug investigation needed
3. Q32 transform tests returning 0 (13 tests) - Likely same root cause as #2
4. Other tests (4 tests) - Individual investigation needed

## Phases

1. Fix JIT tests - Update to use Q32 format
2. Investigate emulator execution bug - Debug why functions return 0
3. Fix emulator execution bug - Implement the fix
4. Verify q32 tests - Should work after phase 3
5. Fix remaining tests - Individual fixes for category 4
6. Cleanup and finalization

## Success Criteria

- All 25 failing tests pass
- No regressions in existing passing tests (64 currently pass)
- Code compiles without warnings
- All code formatted with `cargo +nightly fmt`
