# Arrays Design Notes

## Scope and Goals

Design array support for LPIR (LightPlayer IR) to enable GLSL array compilation on ESP32-C6.
Key capabilities needed:
- Array declarations with explicit sizes
- Element access (constant and dynamic indexing)
- Array assignment (element-wise and whole-array)
- Arrays as function parameters (in, out, inout)
- Arrays of scalars, vectors, and matrices

## Current State

### Old Compiler (lp-glsl-compiler)

**Array Layout:**
- Arrays stored in Cranelift stack slots (`create_sized_stack_slot`)
- Variable info tracks `array_ptr` - a pointer to the stack allocation
- Element size calculated based on type (scalars: 4 bytes, vectors: 4×components, matrices: 4×elements)
- Total allocation = element_size × array_size

**Type System:**
- `Type::Array(Box<Type>, usize)` - element type and size
- Helper methods: `is_array()`, `array_element_type()`, `array_dimensions()`, `array_total_element_count()`

**Declaration & Initialization:**
- Unsized arrays: `int arr[] = {1, 2, 3}` - size inferred from initializer
- Sized arrays: `int arr[5]` - zero-filled if no initializer
- Array initializers stored element-by-element with store instructions

**Element Access:**
- Constant index: `arr[0]` → offset = index × element_size
- Dynamic index: Limited support, compile-time constant only for writes

**Function Parameters:**
- Arrays passed as pointers (by reference) for both `in` and `out`/`inout`
- Non-array `out`/`inout`: stack slot allocated, copy-in (for inout), copy-back after call

### Current LPIR System

**Type Mapping** (`naga_util.rs`):
- `naga_type_to_ir_types()` handles Scalar, Vector, Matrix
- **Array types fall through to unsupported error**

**Slot System** (`lpir/src/`):
- `SlotId`: Index into function's slot table
- `SlotDecl { size }`: Size in bytes
- `SlotAddr { dst, slot }`: Get address of slot
- `Load`/`Store`: Memory operations with offset
- `Memcpy`: Block memory copy

**Lowering Context** (`lower_ctx.rs`):
- `local_map: BTreeMap<Handle<LocalVariable>, VRegVec>` - maps locals to VRegs
- `pointer_args: BTreeMap<u32, Handle<Type>>` - tracks which args are pointers
- **No array metadata tracking currently**

**Access Patterns** (`lower_access.rs`):
- Vectors/matrices: stored in VRegs, dynamic access via `select_lane_dynamic`
- Arrays: need different approach - dynamic indexing requires address computation

## Key Questions

### 1. Array Layout: VRegs vs Stack Slots

**Options:**
- A) Scalarize small arrays to VRegs (like matrices)
- B) Use stack slots uniformly for all arrays

**Analysis:**
- Dynamic indexing requires address computation (base + index × stride)
- VRegs can't be dynamically indexed without select chains (inefficient for larger arrays)
- Stack slots allow proper address computation

**Suggested approach:** Use stack slots uniformly (Option B) as per milestone-iv. The optimizer can promote to VRegs later if beneficial.

### 2. How to Track Array Metadata

**Need to track:**
- Element type (for width calculation)
- Element size in bytes
- Array size (number of elements)
- Slot ID for the allocation

**Options:**
- A) Extend `local_map` values from `VRegVec` to an enum `LocalStorage { VRegs(VRegVec), Array(ArrayInfo) }`
- B) Keep `local_map` as-is for non-arrays, create separate `array_map: BTreeMap<Handle<LocalVariable>, ArrayInfo>`

**Suggested approach:** Option B - separate map keeps non-array path simple and avoids enum overhead for the common case.

### 3. Type Mapping for Arrays

Currently `naga_type_to_ir_types()` returns `IrTypeVec` - a flat list of IR types.

For arrays, we need:
- Declaration: allocate a slot (returns SlotId, not VRegs)
- Access: need to know element layout

**Options:**
- A) Return error for arrays, handle separately in declaration
- B) Return flattened types for small arrays, slot for large arrays
- C) Always use slots, add separate array handling

**Suggested approach:** A - arrays are fundamentally different (pointer-based), handle separately.

### 4. Constant vs Dynamic Indexing

**Constant indexing** (`arr[5]`):
- Compute offset at lowering time: `offset = index × element_size`
- Emit: `SlotAddr` + `Load`/`Store` with constant offset

**Dynamic indexing** (`arr[i]`):
- Runtime computation: `offset = index × element_size`
- Need: multiply op, add op, then load/store
- For stores: need to preserve other elements

**Implementation:**
- `AccessIndex` on array: constant offset → simple load/store
- `Access` on array: dynamic offset → compute address, load/store

### 5. Function Parameters

**Naga representation:**
- `in` array: `TypeInner::Pointer { base: array_type, space: Function }`
- `out`/`inout` array: same pointer type

**Current LPIR:**
- Pointer args get `I32` VReg (the address)
- `pointer_args` map tracks pointee type

**For arrays:**
- Pass array pointer (I32) as argument
- Callee needs to know array dimensions - but GLSL doesn't encode this in type
- **Problem:** How does callee know array bounds for address computation?

**GLSL semantics:**
- Arrays decay to pointers when passed
- No runtime size information carried
- Valid operations: element access, assignment
- Invalid: `.length()` method in some contexts

**Suggested approach:**
- Pass pointer (I32) for all array params
- For constant-index access: caller and callee agree on layout
- For dynamic access: element size is known from element type, but bounds are UB

### 6. Array Initializers

GLSL supports: `int arr[3] = {1, 2, 3}`

**Lowering:**
- Allocate slot
- Emit stores for each element
- Zero-fill remaining elements if partial

### 7. Future Features to Consider

**Globals:**
- Global arrays would use global address space
- Different slot allocation strategy needed
- For now: focus on function-local arrays

**Array constructors:**
- `int arr[3] = int[](1, 2, 3)` - explicit constructor syntax
- Lower same as initializer list

**Multidimensional arrays:**
- `int arr[3][4]` - row-major in GLSL
- Flatten to single slot: total_size = 3 × 4 × element_size
- Index computation: `(i × 4 + j) × element_size`

**Array-of-struct:**
- Out of scope for now (structs not yet implemented)

### 8. Bounds Checking

**GLSL spec:** Out-of-bounds access is undefined behavior.

**Options:**
- A) No bounds check (milestone-iv suggests this)
- B) Clamp index to valid range
- C) Trap on out-of-bounds

**Suggested approach:** A - document as UB, no runtime check in v1. Matches LPIR's existing slot access behavior.

## Open Questions for User

1. **Array layout confirmation:** Stack slots uniformly, or do we want small-array scalarization as a future optimization?

2. **Parameter passing:** For array parameters, how should we handle the callee's knowledge of array dimensions? Options:
   - Trust the programmer (UB on overflow) - simplest
   - Pass hidden size parameter - deviates from GLSL
   - Only support constant indexing in callee for v1?

3. **Multi-dimensional arrays:** Should we support `int arr[3][4]` in initial v1, or defer to later?

4. **Array operations:** Beyond element access, do we need:
   - Whole-array assignment (`arr1 = arr2`)?
   - Array comparison? (GLSL doesn't allow `arr1 == arr2`)
   - `.length()` method?

## Design Decisions (Tentative)

1. **Use stack slots uniformly** for all arrays (scalars, vectors, matrices as elements)
2. **Separate `array_map`** in LowerCtx for array metadata
3. **No bounds checking** in v1 (document as UB)
4. **Pass array pointer** (I32) for function parameters
5. **Support 1D arrays first**, multi-dimensional as follow-up
6. **Element access only** for v1 (no whole-array operations)

## Related Files

- `lp-glsl/lp-glsl-naga/src/lower_ctx.rs` - context with local_map
- `lp-glsl/lp-glsl-naga/src/lower_expr.rs` - expression lowering
- `lp-glsl/lp-glsl-naga/src/lower_stmt.rs` - statement lowering
- `lp-glsl/lp-glsl-naga/src/naga_util.rs` - type mapping
- `lp-glsl/lpir/src/op.rs` - LPIR operations
- `lp-glsl/lpir/src/builder.rs` - slot allocation

## References

- `docs/roadmaps/2026-03-29-lpir-parity/milestone-iv-array-lowering.md` - milestone spec
- Old compiler: `lp-glsl/lp-glsl-compiler/src/frontend/codegen/stmt/declaration.rs`
- Old compiler: `lp-glsl/lp-glsl-compiler/src/frontend/semantic/types.rs`
