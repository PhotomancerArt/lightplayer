# Array Support Implementation Plan

## Overview

Implement comprehensive array support in the GLSL compiler, following the pattern established by increment/decrement operators. Arrays are partially represented in the type system (`Array(Box<Type>, usize)`) but need full implementation across parsing, type checking, code generation, and testing.

## Acceptance Criteria

1. **Full Test Suite**: Comprehensive test coverage for all array features including:
   - Basic array operations (declaration, initialization, indexing, assignment)
   - All math operators on array LValues (see below)
   - Edge cases and error conditions
   - Multi-dimensional arrays
   - Integration with existing features (loops, functions, etc.)

2. **All Math Operators on Array LValues**: Support for all math operators where array elements (`arr[i]`) can be used as:
   - **LValues** (left-hand side, writable): For assignment, increment/decrement, compound assignment
   - **RValues** (right-hand side, readable): For use in expressions, function arguments, return values
   
   Operators that must work with array elements:
   - **Unary operators**: `+arr[i]`, `-arr[i]`, `!arr[i]`, `++arr[i]`, `--arr[i]`, `arr[i]++`, `arr[i]--`
   - **Binary arithmetic operators**: `arr[i] + x`, `arr[i] - x`, `arr[i] * x`, `arr[i] / x`, `arr[i] % x` (and reverse: `x + arr[i]`, etc.)
   - **Binary comparison operators**: `arr[i] == x`, `arr[i] != x`, `arr[i] < x`, `arr[i] > x`, `arr[i] <= x`, `arr[i] >= x`
   - **Binary logical operators**: `arr[i] && x`, `arr[i] || x`, `arr[i] ^^ x`
   - **Assignment operators**: `arr[i] = x`, `arr[i] += x`, `arr[i] -= x`, `arr[i] *= x`, `arr[i] /= x`, `arr[i] %= x`

3. **Build Success**: `scripts/lp-build.sh` must complete without errors after implementation

## Implementation Areas

### 1. Type System Extensions

**Files:**

- `lightplayer/crates/lp-glsl-compiler/src/semantic/types.rs`

**Changes:**

- Add `is_array()` method to `Type` enum
- Add `array_element_type()` method to extract element type
- Add `array_size()` method to get array size
- Update `is_numeric()` to handle arrays of numeric types
- Add helper methods for array type construction

### 2. Type Resolution and Parsing

**Files:**

- `lightplayer/crates/lp-glsl-compiler/src/semantic/type_resolver.rs`
- `lightplayer/crates/lp-glsl-compiler/src/codegen/stmt.rs` (parse_type_specifier)

**Changes:**

- Parse `ArraySpecifier` from GLSL AST to build `Array(Box<Type>, usize)` types
- Handle explicitly-sized arrays (e.g., `float[5]`)
- Support compile-time constant size expressions
- Handle array dimensions in type specifiers (e.g., `vec4[3][2]`)

### 3. Variable Declaration and Storage

**Files:**

- `lightplayer/crates/lp-glsl-compiler/src/codegen/context.rs`
- `lightplayer/crates/lp-glsl-compiler/src/codegen/stmt.rs`

**Changes:**

- Extend `declare_variable()` to handle array types
- Store arrays as multiple variables (one per element) or as a structured storage
- Handle array initialization in declarations
- Support array initializer lists (e.g., `float a[3] = {1.0, 2.0, 3.0}`)

### 4. Array Indexing

**Files:**

- `lightplayer/crates/lp-glsl-compiler/src/codegen/expr/component.rs`
- `lightplayer/crates/lp-glsl-compiler/src/semantic/type_check/inference.rs`

**Changes:**

- Extend `translate_matrix_indexing()` or create `translate_array_indexing()`
- Support runtime array indexing (currently only compile-time constants)
- Handle multi-dimensional arrays (`arr[i][j]`)
- Return element type and values from indexed arrays
- Validate index bounds (compile-time when possible)

### 4a. Runtime Bounds Checking

**Files:**

- `lightplayer/crates/lp-glsl-compiler/src/codegen/expr/component.rs` (or new helper module)

**Implementation Strategy:**

While GLSL spec allows undefined behavior for out-of-bounds array access, we need runtime bounds checking for safety, especially for writes to prevent memory corruption.

**For Array Writes (Required):**

- Check: `index < 0 || index >= array_size`
- Use Cranelift `icmp` to compare index with bounds
- Use `trapnz` or conditional branch with `trap` instruction
- Trap code: Use `TrapCode::user()` to create custom trap code (e.g., `TrapCode::user(1)` for "array out of bounds")
- Generate bounds check before every array element write operation

**For Array Reads (Recommended):**

- Same bounds checking as writes for safety
- Prevents reading invalid memory
- Can be made optional via feature flag if performance is critical

**Code Pattern:**

```rust
// Evaluate index expression
let (index_vals, _) = ctx.translate_expr_typed(index_expr)?;
let index = index_vals[0]; // Must be Int

// Load array size as constant
let array_size = ctx.builder.ins().iconst(types::I32, array_size as i64);

// Check index >= 0
let zero = ctx.builder.ins().iconst(types::I32, 0);
let index_ge_zero = ctx.builder.ins().icmp(IntCC::SignedGreaterThanOrEqual, index, zero);
let index_lt_size = ctx.builder.ins().icmp(IntCC::SignedLessThan, index, array_size);
let in_bounds = ctx.builder.ins().band(index_ge_zero, index_lt_size);

// Trap if out of bounds (using trapnz - trap if condition is non-zero)
let out_of_bounds = ctx.builder.ins().bnot(in_bounds);
let trap_code = TrapCode::user(1).unwrap(); // Custom trap code
ctx.builder.ins().trapnz(out_of_bounds, trap_code);

// Proceed with array access...
```

**Alternative (simpler) approach:**

```rust
// Check index < 0 OR index >= size
let zero = ctx.builder.ins().iconst(types::I32, 0);
let index_lt_zero = ctx.builder.ins().icmp(IntCC::SignedLessThan, index, zero);
let array_size = ctx.builder.ins().iconst(types::I32, array_size as i64);
let index_ge_size = ctx.builder.ins().icmp(IntCC::SignedGreaterThanOrEqual, index, array_size);
let out_of_bounds = ctx.builder.ins().bor(index_lt_zero, index_ge_size);

let trap_code = TrapCode::user(1).unwrap();
ctx.builder.ins().trapnz(out_of_bounds, trap_code);
```

**Optimizations:**

- Skip bounds check for compile-time constant indices (already validated)
- Consider feature flag to disable bounds checks for reads (writes always checked)
- Bounds checks can be optimized away by Cranelift's mid-end in some cases

**Error Handling:**

- Traps are caught by the emulator and converted to `GlslError` with source location
- Trap source location information is preserved for error reporting
- See `lightplayer/crates/lp-glsl-compiler/src/backend/emu.rs` for trap handling

### 5. LValue Support for Arrays

**Files:**

- `lightplayer/crates/lp-glsl-compiler/src/codegen/lvalue.rs`

**Changes:**

- Add `ArrayElement` variant to `LValue` enum:
  ```rust
  ArrayElement {
      base_vars: Vec<Variable>,  // All variables for the array
      base_ty: GlslType,         // Array type
      index: Value,              // Runtime index value (Cranelift Value)
      element_ty: GlslType,      // Element type
  }
  ```
- Update `resolve_lvalue()` to handle `Expr::Bracket` for arrays:
  - Detect when base expression is an array type
  - Evaluate index expression (support runtime indices)
  - Generate bounds checking code for the index
  - Return `LValue::ArrayElement`
- Update `read_lvalue()` to handle `ArrayElement`:
  - Calculate element offset from index
  - Load element value(s) from array variables
  - Handle multi-component elements (vectors, matrices)
- Update `write_lvalue()` to handle `ArrayElement`:
  - Calculate element offset from index
  - Store element value(s) to array variables
  - Validate component count matches
- Update `LValue::ty()` to return element type for `ArrayElement`

**Key Implementation Notes:**

- Array elements use runtime index calculation: `offset = index * element_component_count`
- Bounds checking must be performed before accessing array elements
- For multi-component elements (e.g., `vec4 arr[5]`), each element spans multiple variables

### 6. Increment/Decrement on Array Elements

**Files:**

- `lightplayer/crates/lp-glsl-compiler/src/codegen/expr/incdec.rs`

**Changes:**

- Increment/decrement now works automatically via LValue abstraction!
- `resolve_lvalue()` will handle `arr[i]` expressions and return `LValue::ArrayElement`
- `read_lvalue()` and `write_lvalue()` handle the actual read/write operations
- No special handling needed in `translate_incdec()` - it already uses LValue pattern
- Support `arr[i]++`, `++arr[i]`, `arr[i]--`, `--arr[i]` automatically
- Type checking handled by existing `infer_*_result_type()` functions
- Pre/post increment semantics handled by existing code

### 7. Compound Assignment Operators

**Files:**

- `lightplayer/crates/lp-glsl-compiler/src/codegen/expr/mod.rs` (translate_assignment_typed)
- New file: `lightplayer/crates/lp-glsl-compiler/src/codegen/expr/assignment.rs` (optional)

**Changes:**

- Implement `+=`, `-=`, `*=`, `/=`, `%=` operators
- Support compound assignment on array elements: `arr[i] += value`
- Support compound assignment on variables, components, and array elements
- Type check and coerce operands appropriately
- Follow increment operator pattern for l-value handling

### 8. Array Operations in Expressions

**Files:**

- `lightplayer/crates/lp-glsl-compiler/src/codegen/expr/mod.rs`
- `lightplayer/crates/lp-glsl-compiler/src/codegen/expr/binary.rs`
- `lightplayer/crates/lp-glsl-compiler/src/codegen/expr/unary.rs`
- `lightplayer/crates/lp-glsl-compiler/src/semantic/type_check/operators.rs`

**Changes:**

- Support array elements as operands in all binary operations:
  - Arithmetic: `arr[i] + x`, `arr[i] - x`, `arr[i] * x`, `arr[i] / x`, `arr[i] % x`
  - Comparison: `arr[i] == x`, `arr[i] != x`, `arr[i] < x`, `arr[i] > x`, `arr[i] <= x`, `arr[i] >= x`
  - Logical: `arr[i] && x`, `arr[i] || x`, `arr[i] ^^ x`
- Support array elements in unary operations: `+arr[i]`, `-arr[i]`, `!arr[i]`
- Support array element assignment: `arr[i] = value`
- Handle array-to-array operations (element-wise where applicable)
- Type inference for array indexing expressions
- Ensure array elements work as RValues (read) in all expression contexts

### 9. Array Constructors

**Files:**

- `lightplayer/crates/lp-glsl-compiler/src/codegen/expr/constructor.rs`

**Changes:**

- Support array constructors: `float[5](1.0, 2.0, 3.0, 4.0, 5.0)`
- Support unsized array constructors: `float[](1.0, 2.0, 3.0)`
- Infer array size from constructor arguments when unsized

### 10. Type Checking Extensions

**Files:**

- `lightplayer/crates/lp-glsl-compiler/src/semantic/type_check/inference.rs`
- `lightplayer/crates/lp-glsl-compiler/src/semantic/type_check/operators.rs`

**Changes:**

- Type inference for array indexing: `arr[i]` → element type
- Type checking for array operations
- Validate array sizes in operations
- Check array element types match in assignments

### 11. Validation

**Files:**

- `lightplayer/crates/lp-glsl-compiler/src/semantic/validator.rs`

**Changes:**

- Validate array declarations
- Check array size is positive constant
- Validate array initializers
- Check array bounds in indexing operations

## Test Coverage

**Directory:** `lightplayer/crates/lp-glsl-filetests/filetests/arrays/`

### Basic Array Tests

- `array-declaration.glsl` - Simple array declarations
- `array-initialization.glsl` - Array initialization with initializer lists
- `array-indexing.glsl` - Basic array indexing (read)
- `array-assignment.glsl` - Array element assignment

### Array Operations Tests

**Increment/Decrement Tests (following existing test patterns):**

- `array-preinc-scalar-int.glsl` - Pre-increment on int array element (`++arr[i]`)
  - Pattern: `int arr[3] = {5, 10, 15}; int result = ++arr[1]; // result = 11, arr[1] = 11`
- `array-predec-scalar-int.glsl` - Pre-decrement on int array element (`--arr[i]`)
  - Pattern: `int arr[3] = {5, 10, 15}; int result = --arr[1]; // result = 9, arr[1] = 9`
- `array-preinc-scalar-float.glsl` - Pre-increment on float array element (`++arr[i]`)
  - Pattern: `float arr[3] = {1.5, 2.5, 3.5}; float result = ++arr[1]; // result = 3.5, arr[1] = 3.5`
- `array-predec-scalar-float.glsl` - Pre-decrement on float array element (`--arr[i]`)
  - Pattern: `float arr[3] = {1.5, 2.5, 3.5}; float result = --arr[1]; // result = 1.5, arr[1] = 1.5`
- `array-postinc-scalar-int.glsl` - Post-increment on int array element (`arr[i]++`)
  - Pattern: `int arr[3] = {5, 10, 15}; int result = arr[1]++; // result = 10, arr[1] = 11`
- `array-postdec-scalar-int.glsl` - Post-decrement on int array element (`arr[i]--`)
  - Pattern: `int arr[3] = {5, 10, 15}; int result = arr[1]--; // result = 10, arr[1] = 9`
- `array-postinc-scalar-float.glsl` - Post-increment on float array element (`arr[i]++`)
  - Pattern: `float arr[3] = {1.5, 2.5, 3.5}; float result = arr[1]++; // result = 2.5, arr[1] = 3.5`
- `array-postdec-scalar-float.glsl` - Post-decrement on float array element (`arr[i]--`)
  - Pattern: `float arr[3] = {1.5, 2.5, 3.5}; float result = arr[1]--; // result = 2.5, arr[1] = 1.5`
- `array-preinc-vec2.glsl` - Pre-increment on vec2 array element (`++arr[i]`)
  - Pattern: `vec2 arr[2] = {vec2(1.0, 2.0), vec2(3.0, 4.0)}; vec2 result = ++arr[0]; // component-wise increment`
- `array-predec-vec2.glsl` - Pre-decrement on vec2 array element (`--arr[i]`)
- `array-postinc-vec2.glsl` - Post-increment on vec2 array element (`arr[i]++`)
- `array-postdec-vec2.glsl` - Post-decrement on vec2 array element (`arr[i]--`)
- `array-preinc-component.glsl` - Pre-increment on array element component (`++arr[i].x`)
  - Pattern: `vec2 arr[2] = {vec2(1.0, 2.0), vec2(3.0, 4.0)}; float result = ++arr[0].x; // result = 2.0, arr[0].x = 2.0`
- `array-postinc-component.glsl` - Post-increment on array element component (`arr[i].x++`)
- `array-incdec-edge-cases.glsl` - Edge cases for array increment/decrement
  - Multiple increments: `arr[i]++ + arr[i]++`
  - Nested expressions: `arr[arr[i]++]++`
  - Integer vector arrays: `ivec2 arr[3]; arr[0]++;`

**Other Operator Tests:**

- `array-unary-ops.glsl` - Unary operations on array elements (`+arr[i]`, `-arr[i]`, `!arr[i]`)
- `array-compound-assign.glsl` - Compound assignment operators (`arr[i] += x`, `arr[i] -= x`, `arr[i] *= x`, `arr[i] /= x`, `arr[i] %= x`)
- `array-binary-arithmetic.glsl` - Binary arithmetic with array elements (`arr[i] + x`, `arr[i] - x`, `arr[i] * x`, `arr[i] / x`, `arr[i] % x`)
- `array-binary-comparison.glsl` - Binary comparison with array elements (`arr[i] == x`, `arr[i] != x`, `arr[i] < x`, etc.)
- `array-binary-logical.glsl` - Binary logical with array elements (`arr[i] && x`, `arr[i] || x`, `arr[i] ^^ x`)

### Multi-dimensional Arrays

- `array-multidim.glsl` - Multi-dimensional arrays (`float[3][2]`)
- `array-multidim-indexing.glsl` - Indexing multi-dimensional arrays

### Array Constructors

- `array-constructor.glsl` - Array constructor expressions
- `array-constructor-unsized.glsl` - Unsized array constructors

### Edge Cases and Errors

- `array-bounds-error.glsl` - Out of bounds indexing (compile-time error)
- `array-runtime-bounds-error.glsl` - Runtime out of bounds access (should trap)
- `array-type-mismatch.glsl` - Type mismatch errors
- `array-invalid-init.glsl` - Invalid initializer errors
- `array-incdec-bool.glsl` - Error test: increment/decrement on bool array element (should fail)
  - Pattern: `bool arr[3] = {true, false, true}; arr[0]++; // ERROR: increment not allowed on bool`
- `array-incdec-non-lvalue.glsl` - Error test: increment on array constructor result (should fail)
  - Pattern: `(int[3](1, 2, 3))[0]++; // ERROR: not an lvalue`

### Integration Tests

- `array-in-function.glsl` - Arrays as function parameters/return values
- `array-in-loop.glsl` - Arrays used in loops
- `array-swizzle-combined.glsl` - Arrays combined with vector operations

## Implementation Order

1. **Phase 1: Type System & Parsing**
   - Extend type system with array helper methods
   - Parse array types from AST
   - Basic array declaration support

2. **Phase 2: Array Indexing & LValue Support**
   - Implement array element access
   - Support runtime indexing
   - Implement runtime bounds checking for writes (and reads)
   - Add `ArrayElement` variant to `LValue` enum
   - Update `resolve_lvalue()`, `read_lvalue()`, `write_lvalue()` for arrays
   - Basic array element read/write

3. **Phase 3: Array Operations**
   - Increment/decrement on array elements (works automatically via LValue!)
   - Simple assignment to array elements (works automatically via LValue!)
   - Array initialization

4. **Phase 4: Compound Assignment**
   - Implement all compound assignment operators
   - Support on array elements

5. **Phase 5: Advanced Features**
   - Array constructors
   - Multi-dimensional arrays
   - Array function parameters

6. **Phase 6: Testing**
   - Comprehensive test suite covering all acceptance criteria
   - All math operators on array LValues tested
   - Edge case coverage
   - Error case validation
   - Verify `scripts/lp-build.sh` completes without errors

## Key Implementation Patterns

Following the LValue abstraction pattern (`lvalue.rs`):

1. **LValue Abstraction**: Array elements are handled via `LValue::ArrayElement` variant
   - Unified interface for all modifiable locations (variables, components, matrices, arrays)
   - Increment/decrement and assignment automatically work once LValue is implemented

2. **Type Checking**: Use type inference to determine element types
   - Array element type is the array's element type
   - Type checking happens in `resolve_lvalue()` and operator inference functions

3. **Code Generation**: Use `read_lvalue()` and `write_lvalue()` for all operations
   - Load element: `read_lvalue()` handles offset calculation and loading
   - Store element: `write_lvalue()` handles offset calculation and storing
   - Bounds checking happens in `resolve_lvalue()` before returning `ArrayElement`

4. **Error Handling**: Validate types and bounds with proper error messages
   - Bounds checking generates traps for runtime violations
   - Type mismatches caught during LValue resolution

## Reference Implementation

Use `lightplayer/crates/lp-glsl-compiler/src/codegen/lvalue.rs` as the primary pattern:

- Add `ArrayElement` variant to `LValue` enum (similar to `MatrixElement`)
- Update `resolve_lvalue()` to handle `Expr::Bracket` for arrays (similar to matrix handling)
- Update `read_lvalue()` and `write_lvalue()` to handle `ArrayElement` (similar to `MatrixElement`)
- Increment/decrement (`incdec.rs`) already uses LValue pattern - no changes needed!
- Assignment (`mod.rs`) already uses LValue pattern - no changes needed!

## GLSL Spec References

- Variables spec: `variables.adoc` section `[[arrays]]` (lines 915-1113+)
- Operators spec: `operators.adoc` sections on increment/decrement and assignment
- Operators spec: `operators.adoc` sections on arithmetic, comparison, and logical operators

## Verification Checklist

Before considering the implementation complete, verify:

- [ ] All test files in `lightplayer/crates/lp-glsl-filetests/filetests/arrays/` pass
- [ ] All math operators work on array LValues (unary, binary arithmetic, comparison, logical, assignment)
- [ ] Runtime bounds checking works for array writes
- [ ] Runtime bounds checking works for array reads (or is disabled via feature flag)
- [ ] `scripts/lp-build.sh` completes without errors
- [ ] No regressions in existing tests
- [ ] Code follows existing style and patterns (see `lightplayer/lp-docs/style.md`)






