# Function Call Tests for 32-bit Validation

## Goal

Create comprehensive function call and return value tests for RISC-V32 to:
1. Debug multi-return value issues (like `integer-minmax.clif`)
2. Validate function calling conventions
3. Test argument passing (registers and stack)
4. Test return value handling (single, multiple, mixed register/stack)
5. Ensure proper ABI compliance

## Current Problem

The `integer-minmax.clif` test is failing with `[0, 3, 0]` instead of `[1, 2, 3]`, indicating issues with:
- Multi-return value handling (3 return values: 2 in registers, 1 on stack)
- Register allocation for return values
- Stack return value storage/retrieval

## Test Categories

### Category 1: Basic Function Calls

**Purpose**: Test simple function calls with single return values

**Tests to Copy/Adapt**:
- `runtests/call.clif` - Basic call tests (i64, f64, i8)
- `isa/riscv32/call.clif` - RISC-V32 specific call tests

**Adaptations Needed**:
- Remove f64 tests (requires F/D extensions, defer to Phase 06)
- Keep i8, i16, i32, i64 tests
- Add explicit `target riscv32 has_m` for i64 tests requiring M extension
- Add `set enable_multi_ret_implicit_sret` for multi-return tests

**New Tests to Create**:
- `32bit/runtests/call-basic.clif`:
  - Single i32 return
  - Single i8 return
  - Single i16 return
  - Single i64 return (requires M extension)

### Category 2: Multi-Return Value Tests

**Purpose**: Test functions returning multiple values (critical for debugging current issue)

**Tests to Copy/Adapt**:
- `runtests/integer-minmax.clif` - Already exists, currently failing
- Look for other multi-return tests in main suite

**New Tests to Create**:
- `32bit/runtests/call-multi-return.clif`:
  - Two i32 returns (both in registers: a0, a1)
  - Three i8 returns (two in registers: a0, a1, one on stack)
  - Two i64 returns (requires stack for second)
  - Mixed types: i8, i32, i8 (tests register/stack split)
  - Four i8 returns (tests multiple stack returns)

**Test Structure**:
```clif
test run
set enable_multi_ret_implicit_sret
target riscv32 has_m

; Test: Two i32 returns (both in registers)
function %two_i32(i32, i32) -> i32, i32 {
block0(v0: i32, v1: i32):
    return v0, v1
}
; run: %two_i32(1, 2) == [1, 2]

; Test: Three i8 returns (two registers, one stack)
function %three_i8(i8, i8, i8) -> i8, i8, i8 {
block0(v0: i8, v1: i8, v2: i8):
    return v0, v1, v2
}
; run: %three_i8(1, 2, 3) == [1, 2, 3]
```

### Category 3: Argument Passing Tests

**Purpose**: Test argument passing (registers vs stack)

**Tests to Copy/Adapt**:
- Look for tests with many arguments in main suite
- `runtests/return-call.clif` has tests with many stack arguments

**New Tests to Create**:
- `32bit/runtests/call-args.clif`:
  - Many i32 arguments (test register/stack split at 8 args)
  - Many i8 arguments (test packing)
  - Mixed argument types (i8, i32, i8, i32)
  - i64 arguments (test register pairs)

### Category 4: Call Indirect Tests

**Purpose**: Test indirect function calls

**Tests to Copy/Adapt**:
- `runtests/call_indirect.clif`
- `isa/riscv32/call-indirect.clif`

**Adaptations**:
- Remove f64 tests
- Keep i32, i8, i64 tests
- Add multi-return indirect call tests

### Category 5: Return Call Tests

**Purpose**: Test tail call optimization

**Tests to Copy/Adapt**:
- `runtests/return-call.clif`
- `runtests/return-call-indirect.clif`

**Adaptations**:
- Remove f64 tests
- Focus on i32, i8, i64
- Add multi-return tail call tests

### Category 6: Debugging Tests for Current Issue

**Purpose**: Isolate and debug the `integer-minmax.clif` failure

**New Tests to Create**:
- `32bit/runtests/call-debug-multi-return.clif`:
  - Minimal test: function returning 3 i8 values
  - Test each return value individually
  - Test register return values (first two)
  - Test stack return value (third)
  - Test with different values to see pattern

**Example Debug Test**:
```clif
test run
set enable_multi_ret_implicit_sret
target riscv32 has_m

; Minimal test: return three i8 values
function %minimal_three_i8() -> i8, i8, i8 {
block0:
    v0 = iconst.i8 1
    v1 = iconst.i8 2
    v2 = iconst.i8 3
    return v0, v1, v2
}
; run: %minimal_three_i8() == [1, 2, 3]

; Test: return three i8 values from arguments
function %three_i8_from_args(i8, i8, i8) -> i8, i8, i8 {
block0(v0: i8, v1: i8, v2: i8):
    return v0, v1, v2
}
; run: %three_i8_from_args(1, 2, 3) == [1, 2, 3]

; Test: return computed values
function %three_i8_computed(i8, i8, i8) -> i8, i8, i8 {
block0(v0: i8, v1: i8, v2: i8):
    v3 = iadd v0, v1  ; Should be 3
    v4 = iadd v1, v2  ; Should be 5
    v5 = iadd v0, v2  ; Should be 4
    return v3, v4, v5
}
; run: %three_i8_computed(1, 2, 3) == [3, 5, 4]
```

## Implementation Plan

### Step 1: Copy Existing Call Tests

**Source Files**:
- `cranelift/filetests/filetests/runtests/call.clif`
- `cranelift/filetests/filetests/runtests/call_indirect.clif`
- `cranelift/filetests/filetests/runtests/return-call.clif`
- `cranelift/filetests/filetests/runtests/return-call-indirect.clif`
- `cranelift/filetests/filetests/isa/riscv32/call.clif`
- `cranelift/filetests/filetests/isa/riscv32/call-indirect.clif`

**Target Location**: `cranelift/filetests/filetests/32bit/runtests/`

**Actions**:
1. Copy each file to `32bit/runtests/`
2. Update target to `target riscv32 has_m`
3. Remove f64 tests (defer to Phase 06)
4. Add `set enable_multi_ret_implicit_sret` where needed
5. Verify tests compile and run

### Step 2: Create Multi-Return Test Suite

**New File**: `cranelift/filetests/filetests/32bit/runtests/call-multi-return.clif`

**Test Cases**:

1. **Two Returns (Both Registers)**:
   ```clif
   function %two_i32(i32, i32) -> i32, i32 {
   block0(v0: i32, v1: i32):
       return v0, v1
   }
   ; run: %two_i32(1, 2) == [1, 2]
   ```

2. **Three Returns (Two Registers, One Stack)**:
   ```clif
   function %three_i8(i8, i8, i8) -> i8, i8, i8 {
   block0(v0: i8, v1: i8, v2: i8):
       return v0, v1, v2
   }
   ; run: %three_i8(1, 2, 3) == [1, 2, 3]
   ```

3. **Four Returns (Two Registers, Two Stack)**:
   ```clif
   function %four_i8(i8, i8, i8, i8) -> i8, i8, i8, i8 {
   block0(v0: i8, v1: i8, v2: i8, v3: i8):
       return v0, v1, v2, v3
   }
   ; run: %four_i8(1, 2, 3, 4) == [1, 2, 3, 4]
   ```

4. **Mixed Types**:
   ```clif
   function %mixed_types(i8, i32, i8) -> i8, i32, i8 {
   block0(v0: i8, v1: i32, v2: i8):
       return v0, v1, v2
   }
   ; run: %mixed_types(1, 42, 3) == [1, 42, 3]
   ```

5. **i64 Returns**:
   ```clif
   function %two_i64(i64, i64) -> i64, i64 {
   block0(v0: i64, v1: i64):
       return v0, v1
   }
   ; run: %two_i64(100, 200) == [100, 200]
   ```

### Step 3: Create Debugging Test Suite

**New File**: `cranelift/filetests/filetests/32bit/runtests/call-debug-multi-return.clif`

**Purpose**: Isolate the issue with `integer-minmax.clif`

**Test Cases**:

1. **Minimal Three i8 Returns**:
   - Test with constants only
   - Test with arguments only
   - Test with computed values

2. **Register Return Values**:
   - Test first return value (a0)
   - Test second return value (a1)
   - Verify both are read correctly

3. **Stack Return Value**:
   - Test third return value (on stack)
   - Verify stack offset calculation
   - Verify value is stored correctly

4. **Step-by-Step Debugging**:
   - Start with single return (should work)
   - Add second return (should work)
   - Add third return (currently failing)
   - Isolate which part fails

### Step 4: Create Argument Passing Tests

**New File**: `cranelift/filetests/filetests/32bit/runtests/call-args.clif`

**Test Cases**:

1. **Many i32 Arguments**:
   ```clif
   function %many_i32_args(i32, i32, i32, i32, i32, i32, i32, i32, i32) -> i32 {
   block0(v0: i32, v1: i32, v2: i32, v3: i32, v4: i32, v5: i32, v6: i32, v7: i32, v8: i32):
       v9 = iadd v0, v8
       return v9
   }
   ; run: %many_i32_args(1, 2, 3, 4, 5, 6, 7, 8, 9) == 10
   ```

2. **Many i8 Arguments**:
   - Test argument packing
   - Test sign extension

3. **i64 Arguments**:
   - Test register pairs
   - Test stack arguments for i64

### Step 5: Verify and Document

**Actions**:
1. Run all new tests
2. Document which tests pass/fail
3. Identify patterns in failures
4. Update this plan with findings

## Test Results

### Compilation Status

All test files compile successfully after syntax fixes:

- ✅ `call-basic.clif` - Compiles and passes (1 test)
- ✅ `call-multi-return.clif` - Compiles (fails at runtime due to multi-return bug)
- ✅ `call-debug-multi-return.clif` - Compiles (fails at runtime due to multi-return bug)
- ✅ `call-args.clif` - Compiles (fails at runtime due to multi-return bug)
- ✅ `return-call.clif` - Compiles (fails at runtime due to multi-return bug)
- ⚠️ `call-indirect.clif` - Compiles but fails: `func_addr` instruction not supported on riscv32
- ⚠️ `return-call-indirect.clif` - Compiles but fails: `func_addr` instruction not supported on riscv32

### Runtime Test Results

**Passing Tests:**
- `call-basic.clif` - All single-return call tests pass

**Failing Tests (Expected - Multi-Return Bug):**
- `call-multi-return.clif` - All multi-return tests fail
  - Two-return tests: Return `[100, 0]` instead of `[100, 200]` for i64
  - Three-return tests: Return `[0, 0, 0]` instead of `[1, 2, 3]` for i8
- `call-debug-multi-return.clif` - All debugging tests fail
  - Minimal three i8 returns: Returns `[0, 0, 0]` instead of `[1, 2, 3]`
  - Pattern matches the `integer-minmax.clif` failure
- `call-args.clif` - Tests with multi-return calls fail
- `return-call.clif` - Multi-return tail calls fail

**Known Limitations:**
- `call-indirect.clif` and `return-call-indirect.clif` cannot run on riscv32 because `func_addr` instruction is not supported
- This is a platform limitation, not a bug in the test implementation

### Error Patterns Observed

1. **Multi-Return Value Corruption:**
   - Functions returning 3+ values return zeros for first and third return values
   - Second return value sometimes correct, sometimes corrupted
   - Pattern: `[0, X, 0]` where X may be correct or corrupted

2. **Code Generation Issue:**
   - V-CODE shows values being loaded into wrong registers (e.g., `a4` instead of `a0`, `a1`)
   - Return values not properly moved to ABI-specified return registers
   - Stack return values not stored correctly

3. **Root Cause Confirmed:**
   - Issue is in RISC-V32 code generation (ISLE lowering rules)
   - Not an emulator issue (emulator correctly reads multi-slot returns)
   - Register allocation for return values is incorrect

### Syntax Fixes Applied

1. **Fixed `call_indirect` syntax errors:**
   - Removed invalid typevars (`.i8`, `.i16`) - these are not valid for `call_indirect`
   - Corrected function address placement in multi-return calls
   - Fixed 3 instances in `call-indirect.clif`
   - Fixed 3 instances in `return-call-indirect.clif`

2. **Correct Syntax Pattern:**
   ```clif
   ; Single return
   v2 = call_indirect sig0, v1(v0)
   
   ; Multi-return (no typevar)
   v3, v4 = call_indirect sig0, v2(v0, v1)
   ```

## Test File Structure

```
cranelift/filetests/filetests/32bit/runtests/
├── call-basic.clif              # Basic single-return calls
├── call-multi-return.clif       # Multi-return value tests
├── call-debug-multi-return.clif # Debugging tests for current issue
├── call-args.clif               # Argument passing tests
├── call-indirect.clif           # Indirect call tests (adapted)
├── return-call.clif             # Tail call tests (adapted)
└── return-call-indirect.clif    # Tail indirect call tests (adapted)
```

## Success Criteria

1. ✅ All basic single-return call tests pass
2. ✅ Two-return tests pass (both in registers)
3. ✅ Three-return tests pass (two registers, one stack) - **This is the current blocker**
4. ✅ Four+ return tests pass (multiple stack returns)
5. ✅ i64 return tests pass (register pairs and stack)
6. ✅ Argument passing tests pass (registers and stack)
7. ✅ `integer-minmax.clif` passes or fails with a clearer error

## Debugging Strategy

### Current Issue: `integer-minmax.clif` returns `[0, 3, 0]` instead of `[1, 2, 3]`

**Hypothesis 1**: Register allocation issue - first return value not moved to a0
- **Test**: Create minimal test that returns computed value in a0
- **Expected**: If this fails, it's a register allocation issue

**Hypothesis 2**: Stack return value not stored correctly
- **Test**: Create test that only returns stack value (third return)
- **Expected**: If this fails, it's a storage issue

**Hypothesis 3**: Stack return value read from wrong offset
- **Test**: Create test with known stack offset
- **Expected**: If this fails, it's an offset calculation issue

**Hypothesis 4**: Value computation issue (not a call/return issue)
- **Test**: Test the computation logic separately
- **Expected**: If this passes, issue is in call/return handling

### Debugging Tests to Create

1. **Test Register Returns Only**:
   ```clif
   function %two_returns_only() -> i8, i8 {
   block0:
       v0 = iconst.i8 1
       v1 = iconst.i8 2
       return v0, v1
   }
   ; run: %two_returns_only() == [1, 2]
   ```

2. **Test Stack Return Only**:
   ```clif
   function %three_returns_stack_only() -> i8, i8, i8 {
   block0:
       v0 = iconst.i8 0  ; First return (a0) - should be ignored
       v1 = iconst.i8 0  ; Second return (a1) - should be ignored
       v2 = iconst.i8 42 ; Third return (stack) - this is what we're testing
       return v0, v1, v2
   }
   ; run: %three_returns_stack_only() == [0, 0, 42]
   ```

3. **Test All Three Together**:
   ```clif
   function %three_returns_all() -> i8, i8, i8 {
   block0:
       v0 = iconst.i8 1
       v1 = iconst.i8 2
       v2 = iconst.i8 3
       return v0, v1, v2
   }
   ; run: %three_returns_all() == [1, 2, 3]
   ```

## Next Steps

1. **Immediate**: Create `call-debug-multi-return.clif` with minimal tests
2. **Short-term**: Copy and adapt existing call tests
3. **Medium-term**: Create comprehensive multi-return test suite
4. **Long-term**: Integrate into Phase 02 validation work

## Related Files

- `cranelift/filetests/filetests/32bit/runtests/integer-minmax.clif` - Current failing test
- `lightplayer/crates/lp-riscv-tools/src/emu/emulator.rs` - Emulator return value reading
- `lightplayer/crates/lp-riscv-tools/src/emu/abi_helper.rs` - ABI helper for return locations
- `cranelift/codegen/src/isa/riscv32/abi.rs` - RISC-V32 ABI implementation

## Notes

- The emulator fixes for multi-slot return values are complete
- The remaining issue is likely in code generation (values not moved to correct registers/stacks)
- These tests will help isolate whether the issue is in:
  - Register allocation
  - Stack storage
  - Stack reading (already fixed)
  - Value computation

