# Array Lowering - Stage IV Implementation Notes

## Scope

Implement 1D scalar array support in LPIR for GLSL, covering:

- Phase 1: Foundation (declaration, constant indexing, read/write)
- Phase 2: Bounds checking (dynamic indexing with clamping)
- Phase 3: Initialization (initializer lists, unsized arrays)

Target: Make `array/phase/1-foundation.glsl`, `2-bounds-checking.glsl`, and `3-initialization.glsl`
pass.

## Current State

### Lowering Context (`lower_ctx.rs`)

- `local_map: BTreeMap<Handle<LocalVariable>, VRegVec>` - stores VRegs for non-array locals
- `pointer_args: BTreeMap<u32, Handle<Type>>` - tracks pointer parameters
- **Missing:** Array metadata tracking (slot ID, element info, dimensions)

### Type Mapping (`naga_util.rs`)

- `naga_type_to_ir_types()` handles Scalar, Vector, Matrix
- **Returns error for Array types** - this is expected, arrays need special handling

### Expression Lowering (`lower_expr.rs`)

- `AccessIndex` - handles vector/matrix component access via VRegs
- `Access` - handles dynamic indexing via `select_lane_dynamic` for vectors/matrices
- **Missing:** Array access (needs slot-based addressing)

### Statement Lowering (`lower_stmt.rs`)

- `Store` - handles assignment to locals
- **Missing:** Array element assignment

### LPIR Operations (`lpir/src/op.rs`)

- `SlotAddr { dst, slot }` - get slot base address
- `Load { dst, base, offset }` - read from memory
- `Store { base, offset, value }` - write to memory
- `Memcpy` - block memory copy (for zero-fill)
- **Sufficient for array implementation**

## Key Design Decisions (from `docs/design/arrays.md`)

1. **Stack slots uniformly** - All arrays stored in LPIR slots, not scalarized to VRegs
2. **Fat pointers for parameters** - Pass `(ptr, length)` for array parameters
3. **Bounds clamping** - OOB indices clamp to valid range (v1 safety approach)
4. **Separate `array_map`** - Track array metadata separately from `local_map`

## Questions

### Q1: Should we implement fat pointers in this plan or defer to function parameter phase?

**Context:** Phase 1-3 tests don't include function parameters. Fat pointers are only needed when
arrays are passed to functions. The Phase 7 tests cover array function parameters.

**Options:**

- A) Implement fat pointers now (complete foundation)
- B) Defer fat pointers to Phase 7 (simpler, but revisit array_map design later)

**Suggested:** B) Defer fat pointers. Phase 1-3 focuses on local arrays only. This keeps the plan
focused and allows validating the core slot-based approach first.

### Q2: Bounds check implementation - branch or select?

**Context:** Need to implement clamping:
`clamped = if index < 0 then 0 else if index >= len then len-1 else index`

**Options:**

- A) Branch: `if index < 0 { use 0 } else if index >= len { use len-1 } else { use index }`
- B) Select chain: Use LPIR `Select` ops to compute clamped index without branching

**Suggested:** B) Select chain. Arrays are already using slots (slower than VRegs), and select
chains are easier to generate and optimize later. Branching requires label management.

### Q3: Zero-fill for partial initialization - loop or memset?

**Context:** `int arr[5] = {1, 2};` needs elements 2, 3, 4 set to 0.

**Options:**

- A) Emit individual Store ops for each zero element (simple, fine for small arrays)
- B) Use `Memcpy` from a zeroed temp slot (more efficient for large arrays)

**Suggested:** A) Individual stores for v1. Simpler implementation, and for typical LED shader
arrays (small), the difference is negligible. Can optimize with `Memcpy` later if needed.

## Implementation Phases

1. **Array Declaration & Slot Allocation** - `lower_ctx.rs` changes
2. **Constant Array Access** - `AccessIndex` lowering for arrays
3. **Dynamic Array Access with Clamping** - `Access` lowering with bounds clamping
4. **Array Assignment (Store)** - `Store` through array element pointer
5. **Array Initialization** - Initializer list lowering with zero-fill

## Files to Modify

- `lp-shader/lps-frontend/src/lower_ctx.rs` - Add array_map, slot allocation
- `lp-shader/lps-frontend/src/lower_expr.rs` - Add array Access/AccessIndex
- `lp-shader/lps-frontend/src/lower_stmt.rs` - Add array Store
- `lp-shader/lps-frontend/src/lower_array.rs` - NEW: Array lowering helpers
- `lp-shader/lps-frontend/src/naga_util.rs` - Add array type utilities
- `lp-shader/lps-frontend/src/lib.rs` - Export new module

## Validation

Per phase validation:

```bash
# Phase 1
scripts/glsl-filetests.sh array/phase/1-foundation.glsl

# Phase 2
scripts/glsl-filetests.sh array/phase/2-bounds-checking.glsl

# Phase 3
scripts/glsl-filetests.sh array/phase/3-initialization.glsl

# Full validation
cargo test -p lps-filetests --test filetests -- jit.q32
```

## Notes

- Keep the compiler working at all times - incremental implementation
- Arrays are the first pointer-based storage in LPIR (vectors/matrices use VRegs)
- Test files already exist and have been updated for clamping behavior
