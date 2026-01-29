# Plan: Fix GLSL Test Failures

## Questions

### Q1: JIT Tests - Float Format Rejection

**Context**: All JIT tests (`test_jit_int_literal`, `test_jit_int_addition`, `test_jit_float_literal`, `test_jit_bool_literal`) are failing because they use `DecimalFormat::Float`, which is explicitly rejected in `compile_glsl_to_gl_module_jit()` (line 92-97 of `frontend/mod.rs`).

**Error**: `"Float format is not yet supported. Only Q32 format is currently supported. Float format will cause TestCase relocation errors. Use Q32 format instead."`

**Suggested Answer**: Update all JIT tests to use `DecimalFormat::Q32` instead of `DecimalFormat::Float`. For integer and bool tests, this should work fine. For float tests, we'll need to convert expected values to fixed-point format.

**Decision**: Update all JIT tests to use `DecimalFormat::Q32` instead of `DecimalFormat::Float`. This is the correct format for the current implementation. Float format support is a future TODO.

### Q2: Emulator Tests Returning Zero

**Context**: Multiple emulator tests are returning `0` instead of expected values:
- `test_build_emu_executable`: expects 42, gets 0
- `test_emu_int_literal`: expects 42, gets 0
- `test_emu_int_addition`: expects 30, likely gets 0
- All q32 arithmetic tests: expect various values, get 0

**Pattern**: The tests compile successfully, but function execution returns 0 instead of the expected value.

**Possible Causes**:
1. Function address lookup failing (but should error, not return 0)
2. Function execution failing silently
3. Return value extraction wrong (but should error if wrong type)
4. Function not being called correctly
5. Function returning 0 because execution path is wrong

**Suggested Answer**: Investigate the emulator execution path. Check:
- Are function addresses being populated correctly in `function_addresses`?
- Is `call_function` actually executing the function?
- Are return values being extracted correctly?
- Add debug logging to trace execution

**Investigation Results**:
- 64 tests pass, 25 fail, 4 ignored - so some tests DO work
- Function addresses are populated correctly from symbol map (lines 324-348 in emu.rs)
- Signature lookup uses `cranelift_signatures` HashMap (correct)
- Return value extraction reads from register a0 (register 10)
- The function call path looks correct, but returns 0 instead of expected value

**Hypothesis**: The function may be executing but:
1. Not placing return value in a0 correctly
2. Return value being overwritten after function returns
3. Function not executing at all (but should error, not return 0)
4. Signature mismatch causing wrong return location

**Next Steps**: Add debug logging to trace execution, check if function actually executes, verify return value in a0 register.

### Q3: Q32 Arithmetic Tests

**Context**: All q32 converter tests (`test_q32_fadd`, `test_q32_fsub`, `test_q32_fmul`, `test_q32_fneg`, `test_q32_fabs`, etc.) are returning 0 instead of expected fixed-point values.

**Observation**: The CLIF IR transformation looks correct (we can see the transformed IR in test output), but execution returns 0.

**Suggested Answer**: This is likely the same root cause as Q2 (emulator execution issue). Once we fix the emulator execution, these should work.

**Decision**: This is likely the same root cause as Q2 (emulator execution issue). Once we fix the emulator execution, these should work. The CLIF IR transformation looks correct based on test output.

### Q4: Test Categorization

**Context**: We have 24 failing tests total. Need to categorize them:
1. **JIT Float Format Tests** (4 tests): `test_jit_*` - Need to change to Q32
2. **Emulator Basic Execution** (3 tests): `test_build_emu_executable`, `test_emu_int_*` - Core execution broken
3. **Q32 Transform Tests** (13 tests): All `test_q32_*` - Likely same as #2
4. **Other** (4 tests): `test_do_while`, `test_emu_builtin_sqrt_linked`, `test_emu_float_*`, `test_emu_user_fn_q32` - Need investigation

**Suggested Answer**: 
- Category 1: Simple fix (change DecimalFormat)
- Category 2: Core bug to investigate
- Category 3: Likely same as Category 2
- Category 4: Investigate individually

**Decision**: Fix in order:
1. Category 1 (JIT Float Format) - Simple fix, change DecimalFormat
2. Category 2 (Emulator Basic Execution) - Core bug, needs investigation
3. Category 3 (Q32 Tests) - Should fix automatically once #2 is fixed
4. Category 4 (Other Tests) - Investigate individually after #2 is fixed

No tests should be marked `#[ignore]` unless they're testing future features that aren't implemented yet.
