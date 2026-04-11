# 32-bit Target Validation - Overview

## Goal

Create a comprehensive validator and documentation system for RISC-V32 backend that:

1. Validates CLIF IR before lowering to ensure only supported instructions/types are used
2. Provides clear error messages for unsupported features
3. Documents exactly what is and isn't supported
4. Maintains 32-bit-specific test suite separate from 64-bit tests

## Current State

- RISC-V32 backend supports many CLIF instructions but not all
- Some instructions fail during lowering with unclear errors
- No upfront validation - failures happen late in compilation
- Tests are mixed with 64-bit tests, making it hard to know what's 32-bit specific
- **Initial target**: RV32IMAC (Base + M + A + C extensions)
- Future extensions (F, D, Zba, Zbb, etc.) are architecturally supported but will be rejected if flags aren't set

## Problem Statement

CLIF IR supports:

- 64-bit operations (some supported, some not)
- 128-bit operations (not supported)
- Various instruction types that may or may not be supported on RISC-V32
- Instructions that require specific CPU extensions (F, D, M, etc.)

We need to:

- Catch unsupported instructions early (before lowering)
- Validate that required CPU extensions are enabled for instructions that need them
- Provide clear documentation of what's supported and what extensions are required
- Create a clean 32-bit test suite
- Prevent accidental use of unsupported features

## i64 Instruction Requirements for GLSL Compiler

### Key Findings

Based on analysis of the GLSL compiler codebase and GLSL specification:

- **GLSL only supports 32-bit integers**: The GLSL spec defines only `int` (32-bit signed) and `uint` (32-bit unsigned). There are no 64-bit integer types in GLSL.

- **i64 is only used as an intermediate type**: The GLSL compiler uses i64 exclusively as an intermediate type for fixed-point arithmetic operations (16.16 format) to avoid overflow. All i64 values are computed and then truncated back to i32.

- **No i64 values are stored**: i64 is never used for function parameters, return values, or stored in memory. It's purely an intermediate computation type.

### Required i64 CLIF Instructions

The following i64 instructions are **actually needed** by the GLSL compiler:

#### Type Conversion (Base ISA)

- `sextend` (i32 → i64): Sign-extend i32 to i64
  - Used in: `convert_fmul`, `convert_fdiv`, `convert_fsqrt`
- `ireduce` (i64 → i32): Truncate i64 back to i32
  - Used in: All fixed-point operations to get final i32 result

#### Arithmetic Operations Requiring M Extension

- `imul` (i64 × i64 → i64): Multiplication
  - Used in: `convert_fmul` - multiplying two i32 fixed-point values
- `sdiv` (i64 ÷ i64 → i64): Signed division
  - Used in: `convert_fdiv` and `convert_fsqrt` (Newton-Raphson iterations)

#### Arithmetic Operations (Base ISA)

- `iadd` (i64 + i64 → i64): Addition
  - Used in: Newton-Raphson sqrt iterations
- `smax` (i64, i64 → i64): Maximum of two i64 values
  - Used in: `convert_fsqrt` (ensuring guess ≥ 1)

#### Shift Operations (Base ISA)

- `ishl` (i64 << imm → i64): Left shift
  - Used in: `convert_fdiv` (shifting numerator left by 16), `convert_fsqrt`
- `sshr` (i64 >> imm → i64): Arithmetic right shift
  - Used in: `convert_fmul` (scaling back), `convert_fsqrt` (Newton-Raphson iterations)

### NOT Needed (Can Defer or Skip)

The following i64 operations are **not used** by the GLSL compiler and can be deferred or skipped:

- ❌ i64 load/store operations (values are computed, never stored)
- ❌ i64 comparisons (only used internally, not exposed)
- ❌ i64 bitwise operations (`band`, `bor`, `bxor`, `bnot`)
- ❌ i64 overflow detection (`uadd_overflow`, `sadd_overflow`, etc.)
- ❌ i64 carry operations (`iadd_cout`, `iadd_cin`, `isub_cout`, etc.)
- ❌ i64 immediate arithmetic (`iadd_imm`, `imul_imm` with i64)

### Specific Use Cases

#### 1. Fixed-Point Multiplication (`convert_fmul`)

```
i32 → sextend → i64
i32 → sextend → i64
i64 × i64 → imul → i64
i64 >> 16 → sshr → i64
i64 → ireduce → i32
```

#### 2. Fixed-Point Division (`convert_fdiv`)

```
i32 → sextend → i64
i64 << 16 → ishl → i64
i32 → sextend → i64
i64 ÷ i64 → sdiv → i64
i64 → ireduce → i32
```

#### 3. Fixed-Point Square Root (`convert_fsqrt`)

```
i32 → sextend → i64
i64 << 16 → ishl → i64
[Newton-Raphson iterations using i64 iadd, sdiv, sshr, smax]
i64 → ireduce → i32
```

### Impact on Validation Priorities

For **Phase 03: Arithmetic Instructions**, this analysis shows we should prioritize:

1. ✅ **High Priority**: i64 `imul` and `sdiv` (with M extension validation)
2. ✅ **High Priority**: i64 `sextend` and `ireduce` (type conversions)
3. ✅ **Medium Priority**: i64 `ishl`, `sshr`, `iadd` (arithmetic/shifts)
4. ✅ **Medium Priority**: i64 `smax` (for sqrt)

**Can Defer**:

- i64 load/store validation
- i64 comparison validation
- i64 overflow/carry operation validation
- i64 bitwise operation validation

### Extension Requirements

- **M extension**: Required for i64 `imul` and `sdiv` operations
- **Base ISA (RV32I)**: All other i64 operations (sextend, ishl, sshr, iadd, ireduce, smax)

## Phases

### Phase 01: Infrastructure Setup

**Goal**: Set up validation infrastructure and create 32-bit test directory

**Tasks**:

1. Create validator module structure
2. Define validation trait/interface
3. Create `cranelift/filetests/filetests/32bit/` directory structure
4. Copy and adapt existing tests to be 32-bit specific
5. Revert any changes made to 64-bit tests

**Deliverables**:

- Validator infrastructure in place
- 32-bit test directory with initial tests
- Documentation of validation approach

### Phase 02: Control Flow Instructions

**Goal**: Validate and document control flow instructions

**Instructions to validate**:

- `jump`, `brif`, `br_table`
- `trap`, `trapz`, `trapnz`, `debugtrap`
- `return`
- `call`, `call_indirect`
- `return_call`, `return_call_indirect`

**Deliverables**:

- Validation rules for control flow
- Documentation of supported/unsupported control flow
- Tests for control flow validation

### Phase 03: Arithmetic Instructions

**Goal**: Validate and document arithmetic instructions

**Instructions to validate**:

- `iadd`, `isub`, `imul`
- `sdiv`, `udiv`, `srem`, `urem`
- `iadd_imm`, `imul_imm`
- Overflow variants: `iadd_overflow`, `isub_overflow`, `imul_overflow`
- Narrowing/widening: `iadd_cout`, `iadd_cin`, etc.

**Type considerations**:

- i8, i16, i32: Supported
- i64: Partially supported (needs validation)
- i128: Not supported

**Extension requirements**:

- `imul`, `sdiv`, `udiv`, `srem`, `urem`: Require M extension (`has_m`)
- Basic add/sub: Always supported (RV32I)
- Some i64 optimizations require Zba extension (`has_zba`)

**Deliverables**:

- Validation rules for arithmetic with extension checks
- Documentation of i64 arithmetic support
- Tests for arithmetic validation with/without M extension

### Phase 04: Bitwise Instructions

**Goal**: Validate and document bitwise operations

**Instructions to validate**:

- `band`, `bor`, `bxor`, `bnot`
- `band_imm`, `bor_imm`, `bxor_imm`
- `rotl`, `rotr`
- `bitrev`, `clz`, `ctz`, `popcnt`
- `bselect`, `icmp`

**Deliverables**:

- Validation rules for bitwise operations
- Documentation
- Tests

### Phase 05: Memory Instructions

**Goal**: Validate and document memory operations

**Instructions to validate**:

- `load`, `store`
- `load_complex`, `store_complex`
- `uload8`, `uload16`, `uload32`, `uload64`
- `sload8`, `sload16`, `sload32`, `sload64`
- `istore8`, `istore16`, `istore32`, `istore64`
- Atomic operations (if any)

**Type considerations**:

- 8/16/32-bit loads/stores: Supported
- 64-bit loads/stores: Needs validation
- 128-bit: Not supported

**Deliverables**:

- Validation rules for memory operations
- Documentation
- Tests

### Phase 06: Floating Point Instructions

**Goal**: Validate and document floating point operations

**Instructions to validate**:

- `fadd`, `fsub`, `fmul`, `fdiv`
- `fmin`, `fmax`, `fabs`, `fneg`
- `sqrt`, `ceil`, `floor`, `trunc`, `nearest`
- `fcmp`
- `fma` (fused multiply-add)

**Type and extension considerations**:

- f32: Requires F extension (`has_f`)
- f64: Requires D extension (`has_d`, which requires F)
- f16: Requires Zfh extension (`has_zfh`) for full support, or Zfhmin (`has_zfhmin`) for load/store only
- f128: Not supported

**Extension requirements**:

- All floating point instructions require appropriate extension
- FMA instructions require F or D extension
- Some rounding modes may require Zfa extension

**Deliverables**:

- Validation rules for floating point with extension checks
- Documentation of extension requirements
- Tests for various extension configurations

### Phase 07: Conversion Instructions

**Goal**: Validate and document type conversions

**Instructions to validate**:

- `iconst`, `f32const`, `f64const`
- `sextend`, `uextend`
- `ireduce`, `fpromote`, `fdemote`
- `fcvt_from_sint`, `fcvt_from_uint`
- `fcvt_to_sint`, `fcvt_to_uint`
- `bitcast`, `raw_bitcast`

**Type considerations**:

- Conversions involving i64/i128
- Conversions involving f128
- Vector conversions

**Deliverables**:

- Validation rules for conversions
- Documentation
- Tests

### Phase 08: SIMD/Vector Instructions

**Goal**: Validate and document SIMD operations

**Instructions to validate**:

- Vector arithmetic: `vadd`, `vsub`, `vmul`, etc.
- Vector comparisons: `vicmp`, `vfcmp`
- Vector loads/stores
- Vector shuffles, splats, extracts
- Lane operations

**Status**: Likely not supported on RISC-V32 (unless V extension)

**Deliverables**:

- Validation rules (likely reject all)
- Documentation
- Tests

### Phase 09: Integer Extensions and Reductions

**Goal**: Validate and document integer manipulation

**Instructions to validate**:

- `isplit`, `iconcat`
- `iadd_cout`, `iadd_cin`, `isub_bin`, `isub_bout`
- `iadd_carry`, `isub_carry`
- `umulhi`, `smulhi`

**Deliverables**:

- Validation rules
- Documentation
- Tests

### Phase 10: Special Instructions

**Goal**: Validate and document special/miscellaneous instructions

**Instructions to validate**:

- `nop`
- `select`
- `copy`, `copy_nop`
- `copy_special`
- `stack_addr`, `global_value`
- `get_pinned_reg`, `set_pinned_reg`
- `tls_value`
- `fence`, `fence_atomic`
- `atomic_rmw`, `atomic_cas`
- `table_addr`, `table_size`
- `iconst.i128`, `f128const`

**Deliverables**:

- Validation rules
- Documentation
- Tests

## Validation Approach

### Where Validation Happens

Validation should occur in the `LowerBackend` trait implementation, before lowering begins. We'll add a new method:

```rust
fn validate_function(&self, func: &Function) -> CodegenResult<()>
```

This will be called early in the compilation pipeline, before any lowering attempts.

### CPU Feature Flags

RISC-V32 supports various CPU extensions that must be validated:

**Base Extensions (always present)**:

- **RV32I**: Base integer instruction set (always required)

**Optional Extensions**:

- **RV32M**: Integer multiplication and division (`has_m`)
- **RV32A**: Atomic instructions (`has_a`)
- **RV32C**: Compressed instructions (`has_c`, `has_zca`, `has_zcb`, `has_zcd`)
- **RV32F**: Single-precision floating-point (`has_f`)
- **RV32D**: Double-precision floating-point (`has_d`, requires F)
- **Zba**: Address generation instructions (`has_zba`)
- **Zbb**: Bit manipulation instructions (`has_zbb`)
- **Zbc**: Carry-less multiplication (`has_zbc`)
- **Zbs**: Single-bit instructions (`has_zbs`)
- **Zfa**: Additional floating-point instructions (`has_zfa`)
- **Zfh/Zfhmin**: Half-precision floating-point (`has_zfh`, `has_zfhmin`)
- **Zicond**: Integer conditional operations (`has_zicond`)
- **V**: Vector extension (`has_v`)

**Validation Rules**:

- Floating point instructions (f32) require F extension
- Floating point instructions (f64) require D extension (which requires F)
- Floating point instructions (f16) require Zfh or Zfhmin
- Division/remainder instructions require M extension
- Atomic instructions require A extension
- Vector instructions require V extension
- Various bit manipulation instructions require Zbb/Zba/Zbs extensions

### Validation Structure

```
cranelift/codegen/src/isa/riscv32/validator/
├── mod.rs              # Main validator module
├── instruction.rs       # Instruction validation
├── types.rs            # Type validation
├── control_flow.rs     # Control flow validation
├── arithmetic.rs        # Arithmetic validation
├── memory.rs           # Memory validation
├── floating_point.rs   # FP validation
├── conversions.rs      # Conversion validation
└── simd.rs             # SIMD validation (likely empty)
```

### Error Reporting

Validation errors should:

- Include the instruction/type that's unsupported
- Explain why it's unsupported
- Suggest alternatives if available
- Reference documentation

### Documentation Format

For each instruction category, document:

- **Supported**: List of supported instructions with type constraints
- **Partially Supported**: Instructions with limitations (e.g., i64 division)
- **Unsupported**: Instructions that are not supported
- **Extension Requirements**: Which instructions require which CPU extensions
- **Type-to-Extension Mapping**: Which types require which extensions (e.g., f32 → F, f64 → D)
- **Notes**: Special considerations, extension requirements, etc.

### Extension Requirements Summary

**Initial Target: RV32IMAC**

**Always Required**:

- **RV32I**: Base integer instruction set

**Initially Enabled (RV32IMAC)**:

- **M**: Multiplication/division (`imul`, `sdiv`, `udiv`, `srem`, `urem`) - **ENABLED**
- **A**: Atomic operations - **ENABLED**
- **C**: Compressed instructions - **ENABLED**

**Not Initially Enabled (will be rejected)**:

- **F**: Single-precision floating-point (f32 operations) - **REJECTED**
- **D**: Double-precision floating-point (f64 operations, requires F) - **REJECTED**
- **Zba**: Address generation optimizations - **REJECTED**
- **Zbb**: Bit manipulation instructions - **REJECTED**
- **Zbc**: Carry-less multiplication - **REJECTED**
- **Zbs**: Single-bit instructions - **REJECTED**
- **Zfa**: Additional floating-point instructions - **REJECTED**
- **Zfh/Zfhmin**: Half-precision floating-point (f16) - **REJECTED**
- **Zicond**: Integer conditional operations - **REJECTED**
- **V**: Vector extension - **REJECTED**

**Future Support**: The architecture supports these extensions, but they will be rejected unless explicitly enabled via flags. This allows for future expansion while maintaining strict validation for the initial target.

## Test Structure

```
cranelift/filetests/filetests/32bit/
├── runtests/           # Execution tests (like existing runtests/)
│   ├── arithmetic.clif
│   ├── control-flow.clif
│   ├── memory.clif
│   └── ...
├── isa/riscv32/       # ISA-specific tests
│   └── ...
└── README.md          # Documentation of 32-bit test suite
```

## Success Criteria

1. All unsupported instructions are caught before lowering
2. Clear error messages for unsupported features
3. Complete documentation of supported/unsupported instructions
4. Clean 32-bit test suite separate from 64-bit tests
5. No accidental use of unsupported features in codebase

## Estimated Timeline

- Phase 01: 1-2 days (infrastructure setup)
- Phase 02-10: 1-2 days each (instruction category validation)
- **Total**: ~12-20 days

## Related Work

- Phase 10 from fix-filetests: i64 division/remainder implementation
- This validation work will help identify what truly needs to be implemented vs. what should be rejected upfront
