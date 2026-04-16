# Phase 02: First Adapted Tests and Basic Validation

## Goal

Create the first set of adapted 32-bit Cranelift tests and implement basic validation for foundational instructions. This phase establishes the pattern for adapting tests and demonstrates the validation system working.

## Scope

This phase focuses on:

1. **Basic arithmetic instructions** (iadd, isub, imul, sdiv, udiv, srem, urem)
2. **Basic control flow** (jump, brif, return)
3. **Basic memory operations** (load, store for i32)
4. **Validation tests** (both positive and negative cases)

**Why start here**: These are the most fundamental instructions that form the basis of all programs. They're also relatively simple and don't require many extensions (except M for division/multiplication).

## Tasks

### Task 2.1: Set Up 32-bit Test Directory Structure

**Location**: `cranelift/filetests/filetests/32bit/`

**Directory Structure**:

```
32bit/
├── README.md                    # Documentation of 32-bit test suite
├── runtests/                    # Execution tests (like existing runtests/)
│   ├── arithmetic.clif         # Basic arithmetic operations
│   ├── control-flow.clif       # Basic control flow
│   ├── memory.clif             # Basic memory operations
│   └── validation/             # Tests that verify validation works
│       ├── extension-checks.clif
│       ├── unsupported-types.clif
│       └── unsupported-instructions.clif
└── isa/riscv32/                # ISA-specific tests
    ├── basic.clif              # Basic instruction tests
    └── validation/             # ISA-specific validation tests
        └── extension-requirements.clif
```

**Implementation**:

1. **Create directory structure**:

   ```bash
   mkdir -p cranelift/filetests/filetests/32bit/runtests/validation
   mkdir -p cranelift/filetests/filetests/32bit/isa/riscv32/validation
   ```

2. **Create README.md**:

   ````markdown
   # 32-bit Test Suite

   This directory contains tests specifically adapted for 32-bit targets (primarily RISC-V32).

   ## Structure

   - `runtests/`: Execution tests that compile and run code
   - `isa/riscv32/`: ISA-specific tests for RISC-V32 backend
   - `validation/`: Tests that verify the validator catches unsupported features

   ## Test Adaptation Guidelines

   When adapting tests from the main `runtests/` directory:

   1. **Remove unsupported types**: i128, f128, f64 (unless F/D extensions enabled)
   2. **Change target**: Use `target riscv32` instead of `target riscv64` or `target x86_64`
   3. **Remove 64-bit specific tests**: Tests that rely on 64-bit pointer width
   4. **Update expected results**: Some operations may have different behavior on 32-bit
   5. **Add extension requirements**: Document which extensions are needed

   ## Running Tests

   ```bash
   cargo run --bin clif-util -- test filetests/filetests/32bit/
   ```
   ````

   ```

   ```

### Task 2.2: Copy and Adapt Basic Arithmetic Tests

**Source**: `cranelift/filetests/filetests/runtests/arithmetic.clif`

**Target**: `cranelift/filetests/filetests/32bit/runtests/arithmetic.clif`

**Adaptation Steps**:

1. **Change target**:

   ```clif
   test run
   target riscv32
   ```

2. **Remove unsupported types**:

   - Remove any i128 operations
   - Remove f64 operations (unless we're testing with D extension)
   - Keep i8, i16, i32, i64 (i64 is partially supported)

3. **Remove 64-bit specific tests**:

   - Remove tests that rely on 64-bit pointer width
   - Remove tests that use 64-bit immediates in ways that don't work on 32-bit

4. **Add extension requirements**:
   - Document that division/multiplication require M extension
   - Add comments indicating which tests require M extension

**Example Adapted Test**:

```clif
test run
target riscv32

; Basic arithmetic operations for 32-bit target
; Note: Division and multiplication require M extension (enabled in RV32IMAC)

function %test_iadd(i32, i32) -> i32 {
block0(v0: i32, v1: i32):
    v2 = iadd v0, v1
    return v2
}
; run: %test_iadd(5, 3) == 8

function %test_isub(i32, i32) -> i32 {
block0(v0: i32, v1: i32):
    v2 = isub v0, v1
    return v2
}
; run: %test_isub(10, 3) == 7

; Requires M extension
function %test_imul(i32, i32) -> i32 {
block0(v0: i32, v1: i32):
    v2 = imul v0, v1
    return v2
}
; run: %test_imul(6, 7) == 42

; Requires M extension
function %test_sdiv(i32, i32) -> i32 {
block0(v0: i32, v1: i32):
    v2 = sdiv v0, v1
    return v2
}
; run: %test_sdiv(20, 4) == 5

; Requires M extension
function %test_udiv(i32, i32) -> i32 {
block0(v0: i32, v1: i32):
    v2 = udiv v0, v1
    return v2
}
; run: %test_udiv(20, 4) == 5

; Requires M extension
function %test_srem(i32, i32) -> i32 {
block0(v0: i32, v1: i32):
    v2 = srem v0, v1
    return v2
}
; run: %test_srem(20, 3) == 2

; Requires M extension
function %test_urem(i32, i32) -> i32 {
block0(v0: i32, v1: i32):
    v2 = urem v0, v1
    return v2
}
; run: %test_urem(20, 3) == 2
```

### Task 2.3: Copy and Adapt Basic Control Flow Tests

**Source**: `cranelift/filetests/filetests/runtests/brif.clif`

**Target**: `cranelift/filetests/filetests/32bit/runtests/control-flow.clif`

**Adaptation Steps**:

1. Change target to `riscv32`
2. Remove 64-bit specific tests
3. Keep basic control flow (jump, brif, return)

**Example Adapted Test**:

```clif
test run
target riscv32

; Basic control flow operations for 32-bit target

function %test_jump() -> i32 {
block0:
    jump block1
block1:
    v0 = iconst.i32 42
    return v0
}
; run: %test_jump() == 42

function %test_brif(i32) -> i32 {
block0(v0: i32):
    v1 = iconst.i32 0
    v2 = icmp eq v0, v1
    brif v2, block1, block2
block1:
    v3 = iconst.i32 10
    return v3
block2:
    v4 = iconst.i32 20
    return v4
}
; run: %test_brif(0) == 10
; run: %test_brif(1) == 20

function %test_return(i32) -> i32 {
block0(v0: i32):
    return v0
}
; run: %test_return(42) == 42
```

### Task 2.4: Copy and Adapt Basic Memory Tests

**Source**: `cranelift/filetests/filetests/runtests/` (various memory tests)

**Target**: `cranelift/filetests/filetests/32bit/runtests/memory.clif`

**Adaptation Steps**:

1. Change target to `riscv32`
2. Focus on i32 loads/stores (most common)
3. Remove 64-bit pointer width assumptions

**Example Adapted Test**:

```clif
test run
target riscv32

; Basic memory operations for 32-bit target

function %test_load_store(i32) -> i32 {
block0(v0: i32):
    v1 = stack_slot.i32 4
    store v0, v1
    v2 = load.i32 v1
    return v2
}
; run: %test_load_store(42) == 42
```

### Task 2.5: Implement Basic Validation

**Location**: `cranelift/codegen/src/isa/riscv32/validator/supported.rs`

**Tasks**:

1. **Implement `required_extensions()` for basic instructions**:

   ```rust
   pub fn required_extensions(opcode: Opcode, types: &[Type]) -> Vec<RiscvExtension> {
       let mut extensions = HashSet::new();

       match opcode {
           // Arithmetic requiring M extension
           Opcode::Imul | Opcode::Sdiv | Opcode::Udiv | Opcode::Srem | Opcode::Urem => {
               extensions.insert(RiscvExtension::M);
           },

           // Control flow - no extensions required
           Opcode::Jump | Opcode::Brif | Opcode::Return => {
               // Base ISA
           },

           // Memory - no extensions required for basic load/store
           Opcode::Load | Opcode::Store => {
               // Base ISA (for i32)
           },

           // ... other opcodes
           _ => {
               // Check fallback map
           }
       }

       // Check type-level requirements
       for &ty in types {
           match ty {
               Type::F32 => extensions.insert(RiscvExtension::F),
               Type::F64 => {
                   extensions.insert(RiscvExtension::D);
                   extensions.insert(RiscvExtension::F);
               },
               _ => {}
           }
       }

       extensions.into_iter().collect()
   }
   ```

2. **Add basic opcodes to `SUPPORTED_OPCODES`**:

   ```rust
   const SUPPORTED_OPCODES: &[Opcode] = &[
       // Arithmetic
       Opcode::Iadd,
       Opcode::Isub,
       Opcode::Imul,
       Opcode::Sdiv,
       Opcode::Udiv,
       Opcode::Srem,
       Opcode::Urem,

       // Control flow
       Opcode::Jump,
       Opcode::Brif,
       Opcode::Return,
       Opcode::Call,

       // Memory
       Opcode::Load,
       Opcode::Store,

       // Constants
       Opcode::Iconst,

       // ... more to be added in later phases
   ];
   ```

3. **Implement basic instruction validation**:

   ```rust
   // In validator/instruction.rs

   fn validate_iadd(&self, func: &Function, inst: Inst, data: &InstructionData) -> CodegenResult<()> {
       // iadd is always supported (base ISA)
       Ok(())
   }

   fn validate_sdiv(&self, func: &Function, inst: Inst, data: &InstructionData) -> CodegenResult<()> {
       // sdiv requires M extension (checked in required_extensions)
       // Additional validation: check for division by zero handling
       Ok(())
   }
   ```

### Task 2.6: Create Validation Tests

**Location**: `cranelift/filetests/filetests/32bit/runtests/validation/`

**Purpose**: These tests verify that the validator correctly rejects unsupported features.

**Test Files**:

1. **`extension-checks.clif`**: Tests that instructions requiring extensions are rejected when extensions aren't enabled

```clif
test compile
target riscv32
; Note: M extension not enabled (testing validation)

function %test_sdiv_no_m(i32, i32) -> i32 {
block0(v0: i32, v1: i32):
    v2 = sdiv v0, v1
    return v2
}

; error: Missing required extension M for sdiv
```

2. **`unsupported-types.clif`**: Tests that unsupported types are rejected

```clif
test compile
target riscv32

function %test_i128() -> i128 {
block0:
    v0 = iconst.i128 0
    return v0
}

; error: Unsupported type i128 on riscv32
```

3. **`unsupported-instructions.clif`**: Tests that unsupported instructions are rejected

```clif
test compile
target riscv32

function %test_unsupported() {
block0:
    ; Some instruction that's not supported on riscv32
    ; (Add examples as we discover unsupported instructions)
    return
}

; error: Unsupported instruction on riscv32
```

### Task 2.7: Verify Tests Work

**Steps**:

1. **Run the adapted tests**:

   ```bash
   cd cranelift
   cargo run --bin clif-util -- test filetests/filetests/32bit/runtests/arithmetic.clif
   cargo run --bin clif-util -- test filetests/filetests/32bit/runtests/control-flow.clif
   cargo run --bin clif-util -- test filetests/filetests/32bit/runtests/memory.clif
   ```

2. **Run validation tests**:

   ```bash
   cargo run --bin clif-util -- test filetests/filetests/32bit/runtests/validation/
   ```

3. **Verify validation errors are clear**:
   - Check that error messages mention the missing extension
   - Check that error messages mention the instruction/type that's unsupported
   - Verify that the tests fail with the expected errors

## Success Criteria

1. ✅ 32-bit test directory structure created
2. ✅ At least 3 adapted test files (arithmetic, control-flow, memory)
3. ✅ Basic validation implemented for:
   - Arithmetic instructions (iadd, isub, imul, sdiv, udiv, srem, urem)
   - Control flow (jump, brif, return)
   - Memory (load, store for i32)
4. ✅ Validation tests created that verify:
   - Missing extensions are caught
   - Unsupported types are caught
   - Unsupported instructions are caught
5. ✅ All tests pass (both positive and negative cases)
6. ✅ Error messages are clear and actionable

## Deliverables

1. **Test Files**:

   - `32bit/runtests/arithmetic.clif`
   - `32bit/runtests/control-flow.clif`
   - `32bit/runtests/memory.clif`
   - `32bit/runtests/validation/extension-checks.clif`
   - `32bit/runtests/validation/unsupported-types.clif`
   - `32bit/runtests/validation/unsupported-instructions.clif`
   - `32bit/README.md`

2. **Code**:

   - Updated `validator/supported.rs` with basic instruction support
   - Updated `validator/instruction.rs` with basic validation logic
   - Integration in `riscv32/mod.rs` calling validation

3. **Documentation**:
   - `32bit/README.md` explaining the test suite
   - Comments in test files explaining adaptations

## Estimated Time

- Task 2.1: 1 hour (directory structure)
- Task 2.2: 2-3 hours (adapt arithmetic tests)
- Task 2.3: 1-2 hours (adapt control flow tests)
- Task 2.4: 1-2 hours (adapt memory tests)
- Task 2.5: 3-4 hours (implement basic validation)
- Task 2.6: 2-3 hours (create validation tests)
- Task 2.7: 1-2 hours (verify tests work)
- **Total**: 11-17 hours (~1.5-2 days)

## Notes

- **Start simple**: Focus on the most basic instructions first
- **Test incrementally**: Add one test file at a time and verify it works
- **Document adaptations**: Comment why tests were changed
- **Keep it working**: Don't break existing 64-bit tests

## Next Steps

After Phase 02 is complete:

- Phase 03: Expand to more arithmetic instructions (overflow variants, i64 support)
- Phase 04: Add bitwise instruction validation
- Phase 05: Add more control flow (br_table, call, etc.)
- Phase 06: Add floating point validation (requires F/D extensions)
