# Phase 10: Special Instructions

## Goal

Validate and document special/miscellaneous instructions that don't fit into other categories. This completes the core instruction validation for the RISC-V32 target.

## Select Instructions

### Supported Instructions

- **select**: Conditional selection based on a boolean condition
- **select_spectre_guard**: Same as select but prohibits speculation for Spectre mitigation
- **bitselect**: Bitwise selection: `(c & x) | (~c & y)`

### Supported Types

- **i8, i16, i32**: Supported (base ISA) - **Required for GLSL**
- **i64**: Not supported (rejected in validator due to backend bugs)
- **f32**: Supported with F extension
- **f64**: Supported with D extension
- **i128**: Not supported

### Unsupported Types

- **i128**: Not supported on riscv32 architecture

### Extension Requirements

- **Base ISA (RV32I)**: All integer types (i8, i16, i32, i64)
- **F extension**: Required for f32 operations
- **D extension**: Required for f64 operations (implies F)

### Implementation Notes

#### select
- Uses conditional branches and register moves
- Falls back to branch-based implementation when Zicond extension not available
- Can optimize to conditional moves when Zicond extension is enabled

#### select_spectre_guard
- Semantically identical to select but with different lowering constraints
- Prohibits speculative execution on the controlling value
- Used for Spectre vulnerability mitigation in bounds checks

#### bitselect
- Implemented using bitwise operations: `(c & x) | (~c & y)`
- Does not use conditional branches, providing constant-time execution
- Useful for implementing min/max operations and other conditional logic

### Type-Specific Behavior

#### i64 Operations
- **i64 select is rejected by the validator** due to backend register allocation bugs
- When backend bugs are fixed, i64 select could be implemented by splitting into two i32 operations
- **GLSL does not need i64 select** - all select operations are on i32 values

#### Floating Point Operations
- f32/f64 select follows the same patterns as integer select
- Extension requirements are validated automatically
- Conditional moves are used when available

### Usage Examples

```clif
;; Basic integer select
v3 = select.i32 v0, v1, v2  ; if v0 then v1 else v2

;; Spectre guard select
v3 = select_spectre_guard.i32 v0, v1, v2  ; same but anti-speculation

;; Bitselect
v3 = bitselect.i32 v0, v1, v2  ; (v0 & v1) | (~v0 & v2)

;; Floating point select (requires F extension)
v3 = select.f32 v0, v1, v2  ; if v0 then v1 else v2
```

### Error Messages

#### Unsupported Type
```
Error: Type requires CPU extension on riscv32
Type: i64
Reason: i64 select is not supported on riscv32 (backend bugs)

Error: Type requires CPU extension on riscv32
Type: i128
Reason: i128 select is not supported on riscv32
```

#### Missing Extension
```
Error: Missing required CPU extension on riscv32
Instruction: v3 = select.f32 v0, v1, v2
Required extension: F (Single-precision floating-point (RV32F))
Reason: select requires F extension for f32 operations, but it is not enabled
Suggestion: Enable F extension in target flags, or use integer arithmetic
```

### Test Coverage

Tests are provided in:
- `cranelift/filetests/filetests/32bit/runtests/select.clif` - Valid operations (i32, i16, i8)
- `cranelift/filetests/filetests/32bit/runtests/select-validation-errors.clif` - Error cases (i64, i128 rejection)

### GLSL Compiler Usage

The GLSL compiler uses **only i32 select operations** in fixed-point arithmetic:
- `convert_fabs` uses `select.i32` for absolute value: `if (arg < 0) then -arg else arg`
- `convert_fmax`/`convert_fmin` use `select.i32` for min/max operations
- `convert_sqrt` uses `select.i32` for edge case handling
- All select operations are on **i32 values** - i64 is only used for intermediate arithmetic and reduced to i32 before selection
- **i64 select is rejected by the validator** to prevent backend bugs

## Other Special Instructions

### Nop
- Supported, no extensions required
- Used for padding/debugging

### Trap Instructions
- `trap`, `trapz`, `trapnz`, `debugtrap`: Supported
- Used for runtime assertions and debugging

### Stack Operations
- `stack_addr`: Supported for local variable access
- `stack_load`, `stack_store`: Supported for stack operations

### Atomic Operations
- Require A extension
- `atomic_rmw`, `atomic_cas`: Supported with A extension
- `fence`, `fence_atomic`: Supported for memory ordering

### Global Operations
- `global_value`: Supported for global variable access
- `tls_value`: Supported for thread-local storage

### Function Calls
- `call`, `call_indirect`: Supported
- `return_call`, `return_call_indirect`: Supported for tail calls

## Phase Completion Checklist

- [x] select instruction validation implemented
- [x] select_spectre_guard validation implemented
- [x] bitselect validation implemented
- [x] i128 rejection implemented
- [x] Extension validation for f32/f64 implemented
- [x] Documentation completed
- [x] Tests added
- [x] Integration with validation framework confirmed

## Related Phases

- **Phase 01**: Infrastructure setup provides the validation framework
- **Phase 02-09**: Other instruction categories provide validation patterns
- **Phase 06**: Floating point validation provides extension checking logic

## Future Work

- SIMD/vector select operations (when V extension is supported)
- Additional special instructions as needed
- Performance optimizations for select operations
