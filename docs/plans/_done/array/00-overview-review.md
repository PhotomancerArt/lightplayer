# Array Implementation Plan - Q&A Review

## Questions to Answer

### Phase 0: GLSL AST Structure Understanding

#### Q1: How does the GLSL parser represent array types in TypeSpecifier?

**Answer**:

- `TypeSpecifier` struct has `array_specifier: Option<ArraySpecifier>` field (line 423 in syntax.rs)
- The base type is in `ty: TypeSpecifierNonArray`
- Arrays are represented as: `TypeSpecifier { ty: Float, array_specifier: Some(ArraySpecifier { ... }) }`
- Example: `float[5]` → `TypeSpecifier { ty: Float, array_specifier: Some(ArraySpecifier { dimensions: ... }) }`

**Impact**: Need to check `type_spec.ty.array_specifier` when parsing types.

#### Q2: How are array dimensions represented in ArraySpecifier?

**Answer**:

- `ArraySpecifier` has `dimensions: NonEmpty<ArraySpecifierDimension>` (line 622)
- `ArraySpecifierDimension` is an enum:
  - `Unsized` - for unsized arrays like `float[]`
  - `ExplicitlySized(Box<Expr>)` - for sized arrays like `float[5]` where `5` is an expression
- Multi-dimensional arrays: `float[3][2]` → `ArraySpecifier` with 2 dimensions

**Impact**:

- Need to recursively parse dimensions to build nested `Array(Box<Type>, usize)` types
- Need to evaluate `Expr` for compile-time constant sizes
- Unsized arrays need special handling (infer size from initializer)

#### Q3: How are array initializers represented?

**Answer**:

- `Initializer` enum has two variants (line 739):
  - `Simple(Box<Expr>)` - single value initializer
  - `List(NonEmpty<Initializer>)` - list initializer `{1.0, 2.0, 3.0}`
- Array initializers use nested `List` variants for multi-dimensional arrays
- Example: `float a[3] = {1.0, 2.0, 3.0}` → `Initializer::List([Simple(1.0), Simple(2.0), Simple(3.0)])`

**Impact**:

- Need to handle `Initializer::List` in `emit_initializer()`
- Recursively process nested lists for multi-dimensional arrays
- Validate list length matches array size

#### Q4: How does array indexing work in expressions?

**Answer**:

- `Expr::Bracket(Box<Expr>, ArraySpecifier, SourceSpan)` is used for indexing (line 779)
- The same `ArraySpecifier` structure is used for both type declarations and indexing
- Example: `arr[i]` → `Expr::Bracket(Box<Expr::Variable(arr)>, ArraySpecifier { dimensions: [ExplicitlySized(i)] }, span)`
- Multi-dimensional: `arr[i][j]` → single `Bracket` with 2 dimensions in `ArraySpecifier`

**Impact**:

- Current `translate_matrix_indexing()` already handles `Expr::Bracket` correctly
- Need to extend it to handle arrays (currently rejects non-matrix/vector types)
- The `ArraySpecifier` structure is the same for both type declarations and indexing

#### Q5: Where can arrays be declared?

**Answer**:

- `SingleDeclaration` has `array_specifier: Option<ArraySpecifier>` (line 726)
- `ArrayedIdentifier` also has `array_spec: Option<ArraySpecifier>` (line 487)
- Arrays can be declared in variable declarations, struct fields, function parameters

**Impact**:

- Need to handle arrays in `parse_type_specifier()` for declarations
- Need to handle arrays in struct field parsing
- Need to handle arrays in function parameter parsing

### Phase 1: Type System Extensions

#### Q6: What helper methods are needed for Array types?

**Answer**:
Based on existing patterns (vectors, matrices), need:

- `is_array() -> bool` - check if type is an array
- `array_element_type() -> Option<Type>` - extract element type (recursive for multi-dim)
- `array_size() -> Option<usize>` - get array size (only for single-dim, or first dim)
- `array_dimensions() -> Vec<usize>` - get all dimensions for multi-dim arrays
- Update `is_numeric()` to handle arrays of numeric types recursively

**File**: `lightplayer/crates/lp-glsl-compiler/src/frontend/semantic/types.rs`

**Current State**: `Array(Box<Type>, usize)` exists but no helper methods.

#### Q7: How should `to_cranelift_type()` handle arrays?

**Answer**:

- Arrays should return the element type's Cranelift type (same as matrices)
- Example: `Array(Box<Float>, 5)` → `F32` (not an array type)
- Storage is handled separately (arrays stored as multiple variables)

**File**: `lightplayer/crates/lp-glsl-compiler/src/frontend/semantic/types.rs` (line 157)

### Phase 2: Type Resolution and Parsing

#### Q8: How to parse array types from TypeSpecifier?

**Answer**:

- Check `type_spec.ty.array_specifier` field
- If `Some(ArraySpecifier)`, recursively parse dimensions:
  - Start with base type from `type_spec.ty.ty`
  - For each dimension in `array_specifier.dimensions`:
    - If `Unsized`: error (unsized arrays need initializer to infer size)
    - If `ExplicitlySized(expr)`: evaluate expression to get size, wrap type in `Array`
- Example: `float[5][3]` → `Array(Box<Array(Box<Float>, 3)>, 5)`

**Files**:

- `lightplayer/crates/lp-glsl-compiler/src/frontend/semantic/type_resolver.rs` (line 7)
- `lightplayer/crates/lp-glsl-compiler/src/frontend/codegen/stmt/declaration.rs` (line 137)

**Current State**: Both functions ignore `array_specifier` field.

#### Q9: How to evaluate compile-time constant array sizes?

**Answer**:

- Pattern match `Expr` from `ExplicitlySized(Box<Expr>)` directly
- Check for `Expr::IntConst(n, _)` - extract the constant value
- For non-constants: error (GLSL requires compile-time constant array sizes)
- No general constant evaluator needed - just check for literal constants
- Pattern: `if let Expr::IntConst(n, _) = size_expr.as_ref() { *n as usize } else { error }`

**Reference**: See `component.rs` line 123-127 for existing pattern.

### Phase 3: Variable Declaration and Storage

#### Q10: How should arrays be stored in variables?

**Answer**: **Use stack-allocated memory blocks with pointers** (see `00-array-storage-research.md`)

- **NOT** as individual variables (current pattern for scalars/vectors/matrices)
- Allocate a stack slot using `create_sized_stack_slot()` with size = `array_size * element_size_bytes`
- Get pointer using `stack_addr()`
- Store pointer in `VarInfo` (not `Vec<Variable>`)
- Access elements using `load`/`store` with pointer + offset
- Offset calculation: `offset = index * element_size_bytes`
- For multi-dimensional: `offset = i0 * size1 * size2 * ... + i1 * size2 * ... + i2 * ...`

**Changes Needed**:

- Update `VarInfo` to store `array_ptr: Option<Value>` and `stack_slot: Option<StackSlot>`
- Update `declare_variable()` to allocate stack slot for arrays
- Update `LValue::ArrayElement` to use pointer + offset instead of variable indices

**File**: `lightplayer/crates/lp-glsl-compiler/src/frontend/codegen/context.rs` (line 139)

**Current State**: `declare_variable()` doesn't handle `Type::Array`.

**Reference**: See `lightplayer/plans/array/00-array-storage-research.md` for detailed implementation pattern.

#### Q11: How to handle array initializers?

**Answer**:

- Handle `Initializer::List` in `emit_initializer()`
- For `float a[3] = {1.0, 2.0, 3.0}`:
  - Parse list: `[Simple(1.0), Simple(2.0), Simple(3.0)]`
  - Evaluate each element expression
  - Validate list length matches array size
  - Assign values to array elements sequentially
- For multi-dimensional: recursively process nested lists
- For partial initialization: GLSL allows this, fill remaining with zeros/defaults

**File**: `lightplayer/crates/lp-glsl-compiler/src/frontend/codegen/stmt/declaration.rs` (line 170)

**Current State**: Only handles `Initializer::Simple`.

### Phase 4: Array Indexing

#### Q12: How to extend indexing to support arrays?

**Answer**: **Use both RValue and LValue paths with component selection**

- Extend `emit_indexing()` (already renamed) to handle arrays
- Check array type before matrix/vector check
- **RValue path** (`emit_indexing()` for expressions): Load all components (matches vectors/matrices)
  - Example: `vec3 v = arr[i]` → loads all 3 components (all needed)
- **LValue path** (`resolve_lvalue()` + `read_lvalue()`): Load only needed components
  - Example: `arr[i].x + 1` → loads only `.x` at correct offset (efficient)
- For arrays: calculate offset = `index * element_size_bytes`
- Use `load` with pointer + offset
- Return element type and values

**Decision**: Use both paths - RValue loads all components, LValue loads only needed components (via component selection in `LValue::ArrayElement`).

**File**: `lightplayer/crates/lp-glsl-compiler/src/frontend/codegen/expr/component.rs` (line 63)

**Current State**: Explicitly rejects arrays (line 81-87).

#### Q13: How to handle runtime vs compile-time array indexing?

**Answer**: **Use dynamic offset calculation for all runtime indexing**

- For compile-time constant indices: use `usize` offset (like `VectorElement`)
- For runtime indices: use Cranelift `Value` and calculate offset dynamically:
  - `offset_val = builder.ins().imul(index_val, element_size_const)`
  - Use `load`/`store` with pointer + `offset_val`
- Dynamic offset is simpler and works for any array size
- Select chains only for very small arrays if we want to optimize later

**Decision**: Use dynamic offset calculation for all runtime indexing (simpler, works for any array size).

#### Q14: How to handle multi-dimensional array indexing?

**Answer**: **Calculate flat offset in one step**

- `arr[i][j]` is parsed as single `Bracket` with 2 dimensions
- For `float arr[5][3]` and `arr[i][j]`:
  - `element_offset = i * 3 + j` (outermost-first, per Q2)
  - `byte_offset = element_offset * 4` (float = 4 bytes)
- Use `load`/`store` with pointer + `byte_offset`
- Generalize for N dimensions: `offset = (i0 * size1 + i1) * element_size_bytes` for 2D

**Decision**: Calculate flat offset in one step, generalize for N dimensions.

**File**: `lightplayer/crates/lp-glsl-compiler/src/frontend/codegen/expr/component.rs`

### Phase 5: Runtime Bounds Checking

#### Q15: How to implement runtime bounds checking?

**Answer**: **Always check bounds for writes, check for reads by default**

- Use existing `emit_bounds_check()` pattern from matrices (line 161)
- Check: `index < 0 || index >= array_size`
- Use `icmp` and `trapnz` with `TrapCode::user()`
- Generate bounds check before every array access
- **For writes**: Always check (required for safety)
- **For reads**: Check by default, consider feature flag later if performance is critical

**Decision**: Always check bounds for writes, check for reads by default (can add feature flag later).

**File**: `lightplayer/crates/lp-glsl-compiler/src/frontend/codegen/expr/component.rs`

**Current State**: Pattern exists for matrices, can be reused.

### Phase 6: LValue Support

#### Q16: What LValue variant is needed for arrays?

**Answer**: **Use separate fields for compile-time vs runtime indices**

- Add `ArrayElement` variant to `LValue` enum:
  ```rust
  ArrayElement {
      array_ptr: Value,                    // Pointer to array (from VarInfo)
      base_ty: GlslType,                   // Array type
      index: Option<usize>,                // Compile-time index (if constant)
      index_val: Option<Value>,             // Runtime index (if variable)
      element_ty: GlslType,                 // Element type
      element_size_bytes: usize,           // Element size in bytes
      component_indices: Option<Vec<usize>>, // For component access (like Component)
  }
  ```
- Separate fields allow compile-time optimizations
- `component_indices` supports `arr[i].x` style access

**Decision**: Use separate fields for compile-time vs runtime indices, include component selection support.

**File**: `lightplayer/crates/lp-glsl-compiler/src/frontend/codegen/lvalue.rs` (line 27)

**Current State**: No `ArrayElement` variant exists.

#### Q17: How to update resolve_lvalue() for arrays?

**Answer**: **Handle in Expr::Bracket, component access in Expr::Dot**

- In `Expr::Bracket` handler (line 208), check if base type is array before matrix/vector
- If array: look up array pointer from `VarInfo`, evaluate index expression, generate bounds check, return `ArrayElement`
- Support both compile-time and runtime indices
- Handle multi-dimensional arrays recursively
- `Expr::Dot` handler then extracts components from `ArrayElement` (like it does for `MatrixColumn`)

**Decision**: `resolve_lvalue()` handles `Expr::Bracket` for arrays, `Expr::Dot` extracts components.

**File**: `lightplayer/crates/lp-glsl-compiler/src/frontend/codegen/lvalue.rs` (line 208)

**Current State**: Rejects arrays (line 324-332).

#### Q18: How to implement read_lvalue() for ArrayElement?

**Answer**: **Calculate byte offset, use load with pointer + offset**

- Calculate byte offset: `offset = index * element_size_bytes` (compile-time or runtime)
- If component access: load only needed components at `offset + component_offset`
- If whole element: load all components
- Use `load` with pointer + offset
- Return element type and values

**Decision**: Calculate byte offset, then use `load` with pointer + offset. For component access, load only needed components.

**File**: `lightplayer/crates/lp-glsl-compiler/src/frontend/codegen/lvalue.rs` (line 378)

**Current State**: No `ArrayElement` case in match.

#### Q19: How to implement write_lvalue() for ArrayElement?

**Answer**: **Calculate byte offset, use store with pointer + offset**

- Calculate byte offset from index (compile-time or runtime)
- Generate bounds check before write
- Validate value count matches element component count
- Use `store` for each component at `offset + component_offset`

**Decision**: Calculate byte offset, generate bounds check, then use `store` with pointer + offset for each component.

**File**: `lightplayer/crates/lp-glsl-compiler/src/frontend/codegen/lvalue.rs` (line 444)

**Current State**: No `ArrayElement` case in match.

### Phase 7: Type Checking

#### Q20: How to update type inference for array indexing?

**Answer**:

- In `infer_expr_type_with_registry()` for `Expr::Bracket` (line 206)
- Check if base type is array before matrix/vector
- If array: return element type (recursive for multi-dim)
- Validate index type is `Int`

**File**: `lightplayer/crates/lp-glsl-compiler/src/frontend/semantic/type_check/inference.rs` (line 206)

**Current State**: Rejects arrays (line 210-216).

### Phase 8: Operations

#### Q21: Will increment/decrement work automatically?

**Answer**: **YES** ✅

- `incdec.rs` uses LValue pattern
- Once `ArrayElement` is added to LValue, increment/decrement will work automatically
- No changes needed in `incdec.rs`

**File**: `lightplayer/crates/lp-glsl-compiler/src/frontend/codegen/expr/incdec.rs`

#### Q22: Will compound assignment work automatically?

**Answer**: **YES** ✅ (mostly)

- `assignment.rs` uses LValue pattern (line 157)
- Already supports `+=`, `-=`, `*=`, `/=`
- Missing `%=` operator - need to check if GLSL supports it

**File**: `lightplayer/crates/lp-glsl-compiler/src/frontend/codegen/expr/assignment.rs`

#### Q23: Will binary/unary operations work automatically?

**Answer**: **YES** ✅

- Operations work on RValues
- Once `read_lvalue()` supports `ArrayElement`, array elements can be used as RValues
- Type checking happens in operator inference functions

**Files**:

- `lightplayer/crates/lp-glsl-compiler/src/frontend/codegen/expr/binary.rs`
- `lightplayer/crates/lp-glsl-compiler/src/frontend/codegen/expr/unary.rs`

### Phase 9: Advanced Features

#### Q24: How to implement array constructors?

**Answer**: **Defer to later phase**

- Check if constructor syntax `float[5](1.0, 2.0, 3.0, 4.0, 5.0)` is parsed as `Expr::FunCall`
- Or check if it's a special constructor expression
- Need to verify how GLSL parser handles array constructors
- Return array type with appropriate element type and size

**Decision**: Defer to later phase - check GLSL parser structure first, then implement array constructors.

**File**: `lightplayer/crates/lp-glsl-compiler/src/frontend/codegen/expr/constructor.rs`

**Status**: Need to check if constructor support exists.

#### Q25: How to handle unsized arrays?

**Answer**: **Support unsized arrays** (phase TBD)

- Unsized arrays: `float[]` - size inferred from initializer
- When parsing: if `Unsized` dimension, check initializer
- Infer size from initializer list length
- Convert to sized array type
- Need to handle in type parsing and declaration

**Decision**: Will be supported, but phase to be determined during phase planning.

**Files**: Multiple - need to handle in type parsing and declaration.

### Phase 10: File Paths

#### Q26: What are the correct file paths?

**Answer**: All paths need `frontend/` prefix:

| Plan Path                              | Correct Path                                    |
| -------------------------------------- | ----------------------------------------------- |
| `src/semantic/types.rs`                | `src/frontend/semantic/types.rs`                |
| `src/semantic/type_resolver.rs`        | `src/frontend/semantic/type_resolver.rs`        |
| `src/codegen/context.rs`               | `src/frontend/codegen/context.rs`               |
| `src/codegen/stmt.rs`                  | `src/frontend/codegen/stmt/declaration.rs`      |
| `src/codegen/expr/component.rs`        | `src/frontend/codegen/expr/component.rs`        |
| `src/semantic/type_check/inference.rs` | `src/frontend/semantic/type_check/inference.rs` |
| `src/codegen/lvalue.rs`                | `src/frontend/codegen/lvalue.rs`                |
| `src/codegen/expr/incdec.rs`           | `src/frontend/codegen/expr/incdec.rs`           |
| `src/codegen/expr/mod.rs`              | `src/frontend/codegen/expr/assignment.rs`       |
| `src/semantic/type_check/operators.rs` | `src/frontend/semantic/type_check/operators.rs` |
| `src/semantic/validator.rs`            | `src/frontend/semantic/validator.rs`            |

## Summary of Key Decisions

### ✅ Architecture Decisions

1. **Storage**: Arrays stored as **stack-allocated memory blocks** accessed via **pointers** (not individual variables)

   - Use `create_sized_stack_slot()` and `stack_addr()`
   - Access via `load`/`store` with pointer + offset
   - See `00-array-storage-research.md` for details

2. **Type Parsing**: Unify `parse_type_specifier()` functions, handle arrays recursively

   - Outermost-first dimension order: `float[5][3]` → `Array(Box<Array(Box<Float>, 3)>, 5)`

3. **Indexing**: Use **both RValue and LValue paths**

   - RValue: Load all components (for `vec3 v = arr[i]`)
   - LValue: Load only needed components (for `arr[i].x + 1`)

4. **Runtime Indexing**: Use **dynamic offset calculation** for all runtime indices

   - `offset_val = imul(index_val, element_size_const)`
   - Works for any array size

5. **Bounds Checking**: Always check for writes, check for reads by default (can add feature flag later)

6. **Initialization**: Support **full initialization** per GLSL spec (including partial initialization)

### ✅ Implementation Decisions

- **Q1**: Unify `parse_type_specifier()` functions
- **Q2**: Outermost-first dimension order
- **Q3**: Support partial initialization (per spec)
- **Q4**: Function already renamed to `emit_indexing()`
- **Q5**: Handle variables and function parameters (structs later)
- **Q6**: Add `array_dimensions()` and `array_total_element_count()` (skip `array_size()`)
- **Q7**: Return element type's Cranelift type
- **Q8**: Update unified function to handle arrays recursively
- **Q9**: Literal integers only for Phase 1, note for constant expressions later
- **Q10**: Stack-allocated memory blocks with pointers
- **Q11**: Support full initialization (per spec)
- **Q12**: Both RValue and LValue paths with component selection
- **Q13**: Dynamic offset calculation for all runtime indexing
- **Q14**: Calculate flat offset in one step
- **Q15**: Always check bounds for writes, check for reads by default
- **Q16**: Separate fields for compile-time vs runtime indices
- **Q17**: Handle in `Expr::Bracket`, component access in `Expr::Dot`
- **Q18**: Calculate byte offset, use `load` with pointer + offset
- **Q19**: Calculate byte offset, use `store` with pointer + offset
- **Q20**: Add array type checking before matrix/vector check
- **Q21-Q23**: Will work automatically via LValue pattern ✅
- **Q24**: Defer array constructors to later phase
- **Q25**: Support unsized arrays (phase TBD)
- **Q26**: All file paths need `frontend/` prefix

### ⚠️ Deferred to Later Phases

- Constant expression evaluation for array sizes (beyond literal integers)
- Array constructors (`float[5](1.0, 2.0, ...)`)
- Unsized arrays (phase TBD)
- `%=` operator support (need to verify GLSL spec)

### 📝 Ready for Implementation

- Type system extensions
- Type parsing from AST (with unified function)
- Variable declaration (with stack allocation)
- Array indexing (RValue and LValue paths)
- LValue support (with component selection)
- Type checking updates
- Bounds checking
- Initialization handling

## Next Steps

1. **Organize implementation into phases** based on these decisions
2. **Update main plan** (`00-overview.md`) with correct file paths and decisions
3. **Start Phase 1 implementation** following the Q&A answers
