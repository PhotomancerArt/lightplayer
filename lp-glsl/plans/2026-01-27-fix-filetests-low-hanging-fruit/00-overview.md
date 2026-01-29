# Plan: Fix Filetests Low Hanging Fruit & Test Framework Bugs

## Overview

Fix high-priority bugs identified in filetests failure analysis that prevent tests from running correctly. Focus on test infrastructure bugs and compiler bugs that affect multiple tests.

## Phases

1. Fix test expectations for variable scoping
2. Add precision tolerances to builtin tests
3. Fix equal() function for bvec2 arguments
4. Fix uint() cast for negative values
5. Fix variable scoping implementation
6. Cleanup and verification

## Success Criteria

- All test expectation fixes applied correctly
- Precision tolerances added to all affected tests
- `equal()` function works correctly with bvec2 arguments
- `uint()` cast wraps negative values correctly
- Variable shadowing works correctly in for loops and if blocks
- All affected tests pass
- Code compiles without errors
- No warnings introduced
