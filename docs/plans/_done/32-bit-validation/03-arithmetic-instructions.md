# Phase 03: Expanded Arithmetic Instructions

## Goal

Expand validation and tests for advanced arithmetic instructions beyond the basic operations covered in Phase 02. This includes immediate variants, overflow detection, carry operations, and comprehensive i64 arithmetic support.

## Prerequisites

- Phase 02 completed: Basic arithmetic (iadd, isub, imul, sdiv, udiv, srem, urem) validated and tested
- Phase 01 completed: Validation infrastructure in place

## Scope

This phase focuses on:

1. **Immediate arithmetic instructions** (iadd_imm, isub_imm, imul_imm, etc.)
2. **Overflow detection instructions** (uadd_overflow, sadd_overflow, usub_overflow, ssub_overflow, umul_overflow, smul_overflow)
3. **Carry operations** (iadd_cout, iadd_cin, isub_cout, isub_cin)
4. **Comprehensive i64 arithmetic validation** (documenting what works and what doesn't)
5. **Extension requirements** (Zba for some i64 optimizations)

## Test Adaptation Workflow

**Important**: This phase follows a **copy-and-adapt** workflow:

1. **Copy** original test files from `cranelift/filetests/filetests/runtests/` to `cranelift/filetests/filetests/32bit/runtests/`
2. **Remove** tests that aren't needed for 32-bit:
   - Remove i128 tests (i128 not supported on riscv32)
   - Remove 64-bit-specific tests (tests relying on 64-bit pointer width)
   - Remove tests for unsupported features
3. **Update** target directives to `target riscv32 has_m` (or appropriate extensions)
4. **Keep** i8, i16, i32, i64 tests (all supported on riscv32)
5. **Verify** expected results remain correct after adaptation

This ensures we leverage existing comprehensive tests while making them appropriate for 32-bit targets.

## Tasks

### Task 3.1: Add Immediate Arithmetic Instructions

**Instructions to validate**:

- `iadd_imm`, `isub_imm`, `imul_imm`
- `band_imm`, `bor_imm`, `bxor_imm` (bitwise, but arithmetic-related)
- `ishl_imm`, `ushr_imm`, `sshr_imm` (shift, but arithmetic-related)

**Validation Rules**:

```rust
// In validator/supported.rs

fn opcode_base_extensions(opcode: Opcode) -> Option<Vec<RiscvExtension>> {
    match opcode {
        // ... existing cases ...

        // Immediate arithmetic - same extensions as non-immediate versions
        Opcode::IaddImm | Opcode::IsubImm => Some(vec![]), // Base ISA
        Opcode::ImulImm => Some(vec![RiscvExtension::M]), // Requires M

        // Immediate bitwise - base ISA
        Opcode::BandImm | Opcode::BorImm | Opcode::BxorImm => Some(vec![]),

        // Immediate shifts - base ISA
        Opcode::IshlImm | Opcode::UshrImm | Opcode::SshrImm => Some(vec![]),

        // ... rest of cases ...
    }
}
```

**Special Considerations**:

- `iadd_imm` may use Zba extension for address generation patterns (e.g., `iadd_imm x, 4` for array indexing)
- For now, we'll validate that the instruction is supported, but Zba optimization is handled during lowering
- Immediate values must fit in 12-bit signed range for RISC-V (or use multiple instructions)

**Test File**: `32bit/runtests/arithmetic-imm.clif`

**Note**: If there are existing immediate arithmetic tests in the main `runtests/` directory, copy and adapt them. Otherwise, create new tests based on the patterns below.

**Example Test**:

```clif
test run
target riscv32 has_m

; Basic immediate addition
function %test_iadd_imm(i32) -> i32 {
block0(v0: i32):
    v1 = iadd_imm v0, 42
    return v1
}
; run: %test_iadd_imm(10) == 52

; Immediate subtraction
function %test_isub_imm(i32) -> i32 {
block0(v0: i32):
    v1 = isub_imm v0, 5
    return v1
}
; run: %test_isub_imm(10) == 5

; Immediate multiplication (requires M extension)
function %test_imul_imm(i32) -> i32 {
block0(v0: i32):
    v1 = imul_imm v0, 7
    return v1
}
; run: %test_imul_imm(6) == 42

; Large immediate (may require multiple instructions)
function %test_iadd_imm_large(i32) -> i32 {
block0(v0: i32):
    v1 = iadd_imm v0, 0x1000
    return v1
}
; run: %test_iadd_imm_large(0) == 4096
```

### Task 3.2: Add Overflow Detection Instructions

**Instructions to validate**:

- `uadd_overflow`, `sadd_overflow` (unsigned/signed addition overflow)
- `usub_overflow`, `ssub_overflow` (unsigned/signed subtraction overflow)
- `umul_overflow`, `smul_overflow` (unsigned/signed multiplication overflow)

**Validation Rules**:

```rust
// In validator/supported.rs

fn opcode_base_extensions(opcode: Opcode) -> Option<Vec<RiscvExtension>> {
    match opcode {
        // ... existing cases ...

        // Overflow detection - same extensions as base operations
        Opcode::UaddOverflow | Opcode::SaddOverflow => Some(vec![]), // Base ISA
        Opcode::UsubOverflow | Opcode::SsubOverflow => Some(vec![]), // Base ISA
        Opcode::UmulOverflow | Opcode::SmulOverflow => Some(vec![RiscvExtension::M]), // Requires M

        // ... rest of cases ...
    }
}
```

**Type Considerations**:

- i8, i16, i32: Fully supported
- i64: Supported but requires careful validation (returns two values: result + overflow flag)
- i128: Not supported (will be rejected)

**Test Files** (copy from main `runtests/` and adapt):

**Source Files** (to copy from `cranelift/filetests/filetests/runtests/`):

- `uadd_overflow.clif` → `32bit/runtests/uadd_overflow.clif`
- `sadd_overflow.clif` → `32bit/runtests/sadd_overflow.clif`
- `usub_overflow.clif` → `32bit/runtests/usub_overflow.clif`
- `ssub_overflow.clif` → `32bit/runtests/ssub_overflow.clif`
- `umul_overflow.clif` → `32bit/runtests/umul_overflow.clif` (requires M extension)
- `smul_overflow.clif` → `32bit/runtests/smul_overflow.clif` (requires M extension)

**Copy and Adaptation Steps**:

1. **Copy original test files**:

   ```bash
   cp cranelift/filetests/filetests/runtests/uadd_overflow.clif \
      cranelift/filetests/filetests/32bit/runtests/uadd_overflow.clif
   # Repeat for other overflow test files
   ```

2. **Update target directive**:

   - Change from `target riscv64` or `target x86_64` to `target riscv32 has_m`
   - Keep other targets if they're still relevant (aarch64, s390x, etc.)
   - For multiplication overflow tests, ensure `has_m` is present

3. **Remove unsupported tests**:

   - **Remove i128 tests**: Any functions using `i128` types should be removed
   - **Remove 64-bit specific tests**: Tests that rely on 64-bit pointer width or 64-bit-only behavior
   - **Keep i8, i16, i32, i64 tests**: These are all supported on riscv32

4. **Update test comments**:

   - Add comments explaining i64 overflow behavior (two-register result + overflow flag)
   - Document any riscv32-specific considerations

5. **Verify expected results**:
   - Results should be the same across platforms, but verify i64 overflow detection works correctly

**Example Adapted Test** (from existing `uadd_overflow.clif`):

```clif
test run
target riscv32 has_m

function %uaddof_i32(i32, i32) -> i32, i8 {
block0(v0: i32, v1: i32):
    v2, v3 = uadd_overflow v0, v1
    return v2, v3
}
; run: %uaddof_i32(0, 1) == [1, 0]
; run: %uaddof_i32(0x7FFFFFFF, 1) == [0x80000000, 0]
; run: %uaddof_i32(0xFFFFFFFF, 1) == [0, 1]  ; Overflow!

function %uaddof_i64(i64, i64) -> i64, i8 {
block0(v0: i64, v1: i64):
    v2, v3 = uadd_overflow v0, v1
    return v2, v3
}
; run: %uaddof_i64(0, 0) == [0, 0]
; run: %uaddof_i64(0x7FFFFFFF_FFFFFFFF, 0x80000000_00000001) == [0, 1]  ; Overflow!
```

**Validation Notes**:

- Overflow instructions return two values: the result and an overflow flag (i8)
- For i64, the result is a two-register value, overflow flag is single register
- Validator must check that overflow instructions are not used with i128

### Task 3.3: Add Carry Operations

**Instructions to validate**:

- `iadd_cout`, `iadd_cin` (addition with carry out/in)
- `isub_cout`, `isub_cin` (subtraction with carry out/in)
- `iadd_carry`, `isub_carry` (addition/subtraction with carry)
- `sadd_overflow_cin`, `uadd_overflow_cin` (overflow with carry in)

**Validation Rules**:

```rust
// In validator/supported.rs

fn opcode_base_extensions(opcode: Opcode) -> Option<Vec<RiscvExtension>> {
    match opcode {
        // ... existing cases ...

        // Carry operations - base ISA
        Opcode::IaddCout | Opcode::IaddCin | Opcode::IaddCarry => Some(vec![]),
        Opcode::IsubCout | Opcode::IsubCin | Opcode::IsubCarry => Some(vec![]),
        Opcode::SaddOverflowCin | Opcode::UaddOverflowCin => Some(vec![]),

        // ... rest of cases ...
    }
}
```

**Type Considerations**:

- i8, i16, i32: Supported
- i64: Supported (carry propagates between low and high parts)
- i128: Not supported

**Test File** (copy from main `runtests/` and adapt):

**Source File** (to copy from `cranelift/filetests/filetests/runtests/`):

- `iaddcarry.clif` → `32bit/runtests/arithmetic-carry.clif`

**Copy and Adaptation Steps**:

1. **Copy original test file**:

   ```bash
   cp cranelift/filetests/filetests/runtests/iaddcarry.clif \
      cranelift/filetests/filetests/32bit/runtests/arithmetic-carry.clif
   ```

2. **Update target directive**:

   - Change to `target riscv32 has_m` (or appropriate extensions)
   - Keep other targets if relevant

3. **Remove unsupported tests**:

   - **Remove i128 tests**: Any functions using `i128` types should be removed
   - **Keep i8, i16, i32, i64 tests**: These are all supported on riscv32

4. **Update test comments**:
   - Add comments explaining i64 carry behavior (carry propagates between low/high parts)
   - Document riscv32-specific considerations

**Example Test** (from existing `iaddcarry.clif`):

```clif
test run
target riscv32

function %test_iadd_cout(i32, i32) -> i32, i8 {
block0(v0: i32, v1: i32):
    v2, v3 = iadd_cout v0, v1
    return v2, v3
}
; run: %test_iadd_cout(0xFFFFFFFF, 1) == [0, 1]  ; Carry out

function %test_iadd_cin(i32, i32, i8) -> i32, i8 {
block0(v0: i32, v1: i32, v2: i8):
    v3, v4 = iadd_cin v0, v1, v2
    return v3, v4
}
; run: %test_iadd_cin(0xFFFFFFFE, 1, 1) == [0, 1]  ; With carry in
```

### Task 3.4: Comprehensive i64 Arithmetic Validation

**Goal**: Document and validate what i64 arithmetic operations are supported and how they work.

**Current Status** (from Phase 02):

- ✅ i64 add/sub: Supported (uses two-register pattern)
- ✅ i64 mul: Supported (requires M extension)
- ✅ i64 div/rem: Partially supported (q32-specific optimizations exist)

**Validation Rules**:

```rust
// In validator/instruction.rs

fn validate_i64_arithmetic(&self, func: &Function, inst: Inst, opcode: Opcode) -> CodegenResult<()> {
    // Check that i64 types are allowed
    let data = &func.dfg[inst];
    let result_ty = func.dfg.value_type(func.dfg.first_result(inst));

    match result_ty {
        Type::I64 => {
            // i64 is supported for basic arithmetic
            match opcode {
                Opcode::Iadd | Opcode::Isub | Opcode::Imul => Ok(()),
                Opcode::Sdiv | Opcode::Udiv | Opcode::Srem | Opcode::Urem => {
                    // i64 division is partially supported (q32-specific)
                    // For now, we allow it but document limitations
                    Ok(())
                },
                _ => Err(CodegenError::UserError(format!(
                    "i64 {} not fully supported on riscv32",
                    opcode
                )))
            }
        },
        Type::I128 => {
            Err(ValidationError::UnsupportedType {
                ty: result_ty,
                context: format!("{} instruction", opcode),
            }.into())
        },
        _ => Ok(())
    }
}
```

**Documentation**: Add to `32bit/README.md`:

```markdown
## i64 Arithmetic Support

i64 arithmetic on RISC-V32 uses a two-register pattern (high and low 32-bit parts).

### Supported Operations

- **Addition/Subtraction**: Fully supported using two-register operations
- **Multiplication**: Supported (requires M extension)
- **Division/Remainder**: Partially supported
  - Q32-specific optimizations exist for `(i32 << 16) / i32` patterns
  - General i64 division may not be fully optimized
  - Use with caution for general-purpose i64 division

### Limitations

- i64 operations use two registers (64 bits total)
- Some optimizations may require Zba extension (address generation)
- i64 division is optimized for q32 patterns, not general-purpose
```

**Test File**: `32bit/runtests/i64-arithmetic.clif`

**Example Test**:

```clif
test run
target riscv32 has_m

; i64 addition
function %test_i64_add(i64, i64) -> i64 {
block0(v0: i64, v1: i64):
    v2 = iadd v0, v1
    return v2
}
; run: %test_i64_add(0x00000000_00000001, 0x00000000_00000002) == 3

; i64 multiplication (requires M)
function %test_i64_mul(i64, i64) -> i64 {
block0(v0: i64, v1: i64):
    v2 = imul v0, v1
    return v2
}
; run: %test_i64_mul(0x00000000_00000002, 0x00000000_00000003) == 6
```

### Task 3.5: Update Validator with New Instructions

**Location**: `cranelift/codegen/src/isa/riscv32/validator/supported.rs`

**Changes**:

1. **Add new opcodes to `opcode_base_extensions()`**:

   - Immediate variants
   - Overflow variants
   - Carry variants

2. **Add type validation for overflow instructions**:

   - Ensure i128 is rejected
   - Document i64 support

3. **Add extension requirements**:
   - M extension for multiplication overflow
   - Zba extension notes (for optimization, not validation)

**Example Implementation**:

```rust
pub fn opcode_base_extensions(opcode: Opcode) -> Option<Vec<RiscvExtension>> {
    match opcode {
        // ... existing basic arithmetic ...

        // Immediate arithmetic
        Opcode::IaddImm | Opcode::IsubImm => Some(vec![]),
        Opcode::ImulImm => Some(vec![RiscvExtension::M]),

        // Overflow detection
        Opcode::UaddOverflow | Opcode::SaddOverflow => Some(vec![]),
        Opcode::UsubOverflow | Opcode::SsubOverflow => Some(vec![]),
        Opcode::UmulOverflow | Opcode::SmulOverflow => Some(vec![RiscvExtension::M]),

        // Carry operations
        Opcode::IaddCout | Opcode::IaddCin | Opcode::IaddCarry => Some(vec![]),
        Opcode::IsubCout | Opcode::IsubCin | Opcode::IsubCarry => Some(vec![]),

        // ... rest ...
    }
}
```

### Task 3.6: Create Validation Tests

**Test Files**:

1. **`32bit/runtests/validation/overflow-requires-m.clif`**:

   ```clif
   test compile
   target riscv32
   ; Note: M extension not enabled

   function %test_umul_overflow_no_m(i32, i32) -> i32, i8 {
   block0(v0: i32, v1: i32):
       v2, v3 = umul_overflow v0, v1
       return v2, v3
   }

   ; error: Missing required extension M for umul_overflow
   ```

2. **`32bit/runtests/validation/i128-arithmetic.clif`**:

   ```clif
   test compile
   target riscv32

   function %test_i128_add(i128, i128) -> i128 {
   block0(v0: i128, v1: i128):
       v2 = iadd v0, v1
       return v2
   }

   ; error: Unsupported type i128 on riscv32
   ```

3. **`32bit/runtests/validation/immediate-range.clif`**:

   ```clif
   test run
   target riscv32

   ; Test that large immediates work (may use multiple instructions)
   function %test_large_imm(i32) -> i32 {
   block0(v0: i32):
       v1 = iadd_imm v0, 0x7FFFFFFF
       return v1
   }
   ; run: %test_large_imm(1) == 0x80000000
   ```

### Task 3.7: Adapt Additional Overflow Test Variants

**Additional Source Files** (from main `runtests/` to copy and adapt):

- `uadd_overflow_narrow.clif` → `32bit/runtests/uadd_overflow_narrow.clif`
- `uadd_overflow_trap.clif` → `32bit/runtests/uadd_overflow_trap.clif`
- `uadd_overflow_128.clif` → **Skip** (i128 not supported, don't copy)

**Copy and Adaptation Steps**:

1. **Copy original test files**:

   ```bash
   cp cranelift/filetests/filetests/runtests/uadd_overflow_narrow.clif \
      cranelift/filetests/filetests/32bit/runtests/uadd_overflow_narrow.clif
   cp cranelift/filetests/filetests/runtests/uadd_overflow_trap.clif \
      cranelift/filetests/filetests/32bit/runtests/uadd_overflow_trap.clif
   ```

2. **Update target directive**:

   - Change to `target riscv32 has_m` (or appropriate extensions)
   - Remove targets that don't support riscv32

3. **Remove unsupported tests**:

   - **Remove i128 tests**: Any functions using `i128` types should be removed
   - **Remove 64-bit specific tests**: Tests that rely on 64-bit pointer width
   - **Keep narrow type tests**: i8, i16 tests are valuable for riscv32
   - **Keep trap tests**: Trap variants should work on riscv32

4. **Verify trap behavior**:
   - Ensure trap tests work correctly on riscv32
   - Update comments if riscv32 trap behavior differs

**Note**: Task 3.2 already covers the main overflow test files. This task covers the additional variants (narrow, trap) that provide more comprehensive coverage.

## Success Criteria

1. ✅ Immediate arithmetic instructions validated and tested
2. ✅ Overflow detection instructions validated and tested
3. ✅ Carry operations validated and tested
4. ✅ i64 arithmetic comprehensively documented
5. ✅ All adapted tests pass
6. ✅ Validation tests correctly reject unsupported features
7. ✅ Extension requirements properly enforced

## Deliverables

1. **Test Files**:

   - `32bit/runtests/arithmetic-imm.clif`
   - `32bit/runtests/uadd_overflow.clif`
   - `32bit/runtests/sadd_overflow.clif`
   - `32bit/runtests/usub_overflow.clif`
   - `32bit/runtests/ssub_overflow.clif`
   - `32bit/runtests/umul_overflow.clif`
   - `32bit/runtests/smul_overflow.clif`
   - `32bit/runtests/arithmetic-carry.clif`
   - `32bit/runtests/i64-arithmetic.clif`
   - `32bit/runtests/validation/overflow-requires-m.clif`
   - `32bit/runtests/validation/i128-arithmetic.clif`

2. **Code**:

   - Updated `validator/supported.rs` with new opcodes
   - Updated `validator/instruction.rs` with i64 validation
   - Documentation in `32bit/README.md`

3. **Documentation**:
   - i64 arithmetic support documentation
   - Extension requirements for overflow operations

## Estimated Time

- Task 3.1: 2-3 hours (immediate arithmetic)
- Task 3.2: 4-5 hours (overflow detection - multiple test files)
- Task 3.3: 2-3 hours (carry operations)
- Task 3.4: 3-4 hours (i64 validation and documentation)
- Task 3.5: 2-3 hours (validator updates)
- Task 3.6: 2-3 hours (validation tests)
- Task 3.7: 3-4 hours (adapting existing tests)
- **Total**: 18-25 hours (~2.5-3 days)

## Notes

- **Overflow Instructions**: These return two values (result + overflow flag). For i64, this means 3 total values (2 for result, 1 for flag). Ensure the validator handles multi-return values correctly.

- **i64 Division**: Current implementation has q32-specific optimizations. Document this limitation clearly.

- **Zba Extension**: Some i64 optimizations may use Zba, but this is an optimization, not a requirement. Validation should not require Zba for basic i64 operations.

- **Immediate Range**: RISC-V immediates are 12-bit signed. Larger immediates require multiple instructions. The validator doesn't need to check immediate ranges (that's a lowering concern), but tests should verify large immediates work.

## Next Steps

After Phase 03 is complete:

- Phase 04: Bitwise instruction validation
- Phase 05: Memory instruction validation
- Phase 06: Floating-point instruction validation
