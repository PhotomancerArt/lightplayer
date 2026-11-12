# Phase 2: Investigate Emulator Execution Bug

## Description

Investigate why emulator tests are returning 0 instead of expected values. The function execution path appears correct, but return values are wrong.

## Tests Affected

- `backend::codegen::emu::tests::test_build_emu_executable` - expects 42, gets 0
- `exec::emu::tests::test_emu_int_literal` - expects 42, gets 0
- `exec::emu::tests::test_emu_int_addition` - expects 30, likely gets 0
- All q32 arithmetic tests (13 tests) - all return 0

## Investigation Steps

1. Add debug logging to `call_i32` to trace:
   - Function address lookup
   - Function signature retrieval
   - Arguments being passed
   - Return values from `call_function`
   - Register a0 value after function call

2. Add debug logging to `build_emu_executable` to verify:
   - Function addresses are populated correctly
   - Cranelift signatures are stored correctly
   - Symbol map contains expected functions

3. Check if function actually executes:
   - Add logging in emulator `call_function` to trace execution
   - Verify PC moves through function code
   - Check if function returns correctly

4. Verify return value extraction:
   - Check if return value is in register a0 after function returns
   - Verify `extract_return_values` is reading correct register
   - Check if return location computation is correct

## Possible Root Causes

1. Function not executing (but should error, not return 0)
2. Return value not placed in a0 correctly
3. Return value overwritten after function returns
4. Signature mismatch causing wrong return location
5. Function address incorrect (but should error on lookup)

## Success Criteria

- Root cause identified
- Evidence collected (logs, register dumps, etc.)
- Fix approach determined
