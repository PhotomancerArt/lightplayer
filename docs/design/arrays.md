# Array Support in LPIR

## Overview

This document describes how GLSL arrays are represented, stored, and accessed in LPIR (LightPlayer IR). Arrays are a fundamental GLSL feature that enables collections of values with dynamic indexing.

## Goals

- Support 1D arrays of scalars, vectors, and matrices
- Support multi-dimensional arrays (e.g., `int arr[3][4]`)
- Support constant and dynamic element indexing
- Support arrays as function parameters (`in`, `out`, `inout`)
- Support array initializers (`int arr[3] = {1, 2, 3}`)
- Provide memory safety on embedded targets via bounds checking
- Lay groundwork for future `.length()` method

## Non-Goals

- Array-of-struct (depends on struct support)
- Array equality operators (`arr1 == arr2` - not valid GLSL)
- Dynamic array sizing (`int arr[]` without initializer)
- Runtime resizable arrays

## Background

### Why Arrays Need Special Handling

Unlike vectors and matrices (which have fixed, small sizes and can be scalarized to VRegs), arrays:
1. Can be arbitrarily large (e.g., `float data[1000]`)
2. Require dynamic indexing at runtime (`arr[i]` where `i` is unknown at compile time)
3. Need address computation: `base + index × element_size`

### Safety Requirements on Embedded

On ESP32-C6, shaders run in the same address space as the firmware. An out-of-bounds array access could corrupt the runtime. This requires:
1. Runtime bounds checking on dynamic indexing
2. Defined behavior on OOB (not just "undefined behavior")

## Design

### Array Layout: Stack Slots

All arrays are stored in LPIR stack slots, allocated via `FunctionBuilder::alloc_slot(size_in_bytes)`.

**Layout:**
```
Slot memory (row-major for multi-dimensional):
┌─────────────────────────────────────────────────────────────┐
│ element 0 │ element 1 │ element 2 │ ... │ element N-1      │
└─────────────────────────────────────────────────────────────┘
           ↑                                           ↑
         base address                              base + size
```

**Element size calculation:**
- Scalar (int/float/bool): 4 bytes
- Vector (vecN/ivecN/bvecN): 4 × N bytes
- Matrix (matNxM): 4 × N × M bytes

**Multi-dimensional array layout:**
```glsl
int arr[3][4];  // Row-major: 12 elements contiguous
```
Flat index: `i × 4 + j` for `arr[i][j]`

### Array Metadata

For each array-typed local variable, we track:

```rust
struct ArrayInfo {
    slot: SlotId,           // LPIR slot allocation
    element_type: TypeInfo, // Element type info
    element_size: u32,      // Size in bytes
    element_count: u32,     // Number of elements
    dimensions: Vec<u32>,   // Per-dimension sizes (empty for 1D)
}
```

Storage in `LowerCtx`:
```rust
pub(crate) array_map: BTreeMap<Handle<LocalVariable>, ArrayInfo>
```

### Type Mapping

Arrays are handled separately from the `naga_type_to_ir_types()` pipeline. When lowering a local variable declaration:

1. Check if type is array via `TypeInner::Array`
2. Calculate total size: `element_count × element_size`
3. Allocate slot: `fb.alloc_slot(total_size)`
4. Store in `array_map` (not `local_map`)

### Element Access

#### Constant Indexing (`arr[5]`)

For `Expression::AccessIndex { base, index }` on array:

1. Compute byte offset: `index × element_size`
2. Emit `SlotAddr` to get base pointer
3. Emit `Load`/`Store` with constant offset:
   ```
   v_base = SlotAddr(slot)
   v_val = Load(v_base, offset=20)   // arr[5] where element_size=4
   ```

#### Dynamic Indexing (`arr[i]`)

For `Expression::Access { base, index }` on array:

1. Emit bounds check (see below)
2. Compute runtime offset:
   ```
   v_base = SlotAddr(slot)
   v_offset_bytes = Imul(index_v, element_size)
   v_addr = Iadd(v_base, v_offset_bytes)
   v_val = Load(v_addr, offset=0)
   ```

### Bounds Checking

**Fat pointer representation for parameters:**
Arrays passed as function parameters use a "fat pointer" - two I32 values:
1. **Pointer** - base address (I32)
2. **Length** - element count (I32)

This matches Rust slice representation (`&[T]` = ptr + len).

**Bounds check lowering:**
```
// Before dynamic array access:
v_in_bounds = IltU(index_v, length_v)      // index < length
BrIfNot(v_in_bounds) -> oob_handler

// OOB handler (v1: clamp, future: trap)
v_clamped = Select(v_in_bounds, index_v, Iconst(0))  // or length-1
```

**v1 behavior:** Clamp to valid range (0 to length-1). Document as limitation.

**Future behavior:** Trap to error handler with stack unwinding for recovery.

### Function Parameters

**Parameter passing:**

| Param Type | Representation | Registers |
|------------|----------------|-----------|
| `in` scalar/vector/matrix | By value (flattened) | N VRegs |
| `in` array | Fat pointer | 2 VRegs (ptr, length) |
| `out`/`inout` non-array | Pointer | 1 VReg (ptr) |
| `out`/`inout` array | Fat pointer | 2 VRegs (ptr, length) |

**Callee handling:**

For array parameters, the callee receives the fat pointer and uses it for access:

```rust
// In LowerCtx::new()
for (i, arg) in func.arguments.iter().enumerate() {
    let inner = &module.types[arg.ty].inner;
    match inner {
        TypeInner::Pointer { base, .. } => {
            let base_inner = &module.types[*base].inner;
            if is_array_type(base_inner) {
                // Fat pointer: 2 VRegs
                let ptr_vreg = fb.add_param(IrType::I32);
                let len_vreg = fb.add_param(IrType::I32);
                arg_vregs.insert(i as u32, smallvec![ptr_vreg, len_vreg]);
                // Store array info for bounds checking
                array_params.insert(i as u32, compute_array_info(*base));
            } else {
                // Regular pointer (non-array)
                let addr = fb.add_param(IrType::I32);
                arg_vregs.insert(i as u32, smallvec![addr]);
                pointer_args.insert(i as u32, *base);
            }
        }
        // ... non-pointer handling
    }
}
```

### Array Initializers

GLSL: `int arr[3] = {1, 2, 3}`

Lowering:
1. Allocate slot for array
2. For each initializer element:
   - Compute offset: `index × element_size`
   - Emit store of initializer value
3. If partial initialization (fewer elements than array size):
   - Zero-fill remaining elements (memcpy from zeroed temp or loop)

### Multi-Dimensional Arrays

**Declaration:** `int arr[3][4]`

Storage: Single flat slot with 12 elements (row-major).

**Indexing:** `arr[i][j]`

Lowering computes flat index:
```
flat_index = i × 4 + j           // inner dimension is 4
byte_offset = flat_index × 4    // element_size for int
```

For N-dimensional arrays:
```
// arr[d0][d1][d2]...[dn-1]
// flat_index = i0×(d1×d2×...×dn) + i1×(d2×...×dn) + ... + in-1
```

## Architecture

### File Structure

```
lp-glsl/lp-glsl-naga/src/
├── lower_ctx.rs          # Add array_map, fat pointer handling
├── lower_expr.rs         # Add Access/AccessIndex for arrays
├── lower_stmt.rs         # Add array Store handling
├── lower_array.rs        # NEW: Array-specific lowering helpers
├── naga_util.rs          # Add array type utilities
└── mod.rs                # Export new modules

lp-glsl/lpir/src/
├── op.rs                 # Existing: SlotAddr, Load, Store, Memcpy
├── builder.rs            # Existing: alloc_slot()
└── types.rs              # Existing: SlotId
```

### Component Interaction

```
Naga GLSL parsing
       │
       ▼
┌─────────────────────┐
│ Array Type Detection│  TypeInner::Array
└──────────┬──────────┘
           │
           ▼
┌─────────────────────┐
│ ArrayInfo creation  │  Compute element size, dimensions, total size
└──────────┬──────────┘
           │
           ▼
┌─────────────────────┐
│ Slot Allocation     │  fb.alloc_slot(total_size)
└──────────┬──────────┘
           │
           ▼
┌─────────────────────┐     ┌─────────────────────┐
│ array_map storage   │◄────│ Local variable use  │
└──────────┬──────────┘     └─────────────────────┘
           │
           ▼
┌─────────────────────┐
│ Element Access      │  ┌────────────────────────────┐
│ - Constant index    │  │ AccessIndex: offset = index × elem_size  │
│ - Dynamic index     │  │ Access: offset = runtime compute          │
└──────────┬──────────┘  └────────────────────────────┘
           │
           ▼
┌─────────────────────┐
│ Bounds Check        │  ┌────────────────────────────┐
│ (dynamic only)      │  │ index < length, clamp if OOB (v1)         │
└──────────┬──────────┘  └────────────────────────────┘
           │
           ▼
┌─────────────────────┐
│ LPIR Load/Store     │  SlotAddr + offset computation
└─────────────────────┘
```

## Decisions

### 1. Stack Slots Uniformly (Not Scalarization)

**Decision:** Use stack slots for all arrays, not just large ones.

**Rationale:**
- Dynamic indexing requires address computation; VRegs can't be dynamically indexed efficiently
- Uniform approach is simpler to implement and reason about
- Optimizer can promote to VRegs later if profiling shows benefit
- Matches the milestone-iv recommendation

### 2. Fat Pointers with Element Count

**Decision:** Pass `(ptr, element_count)` as fat pointer for array parameters.

**Rationale:**
- Prior art: Rust slices, Go slices, C++ std::span all use element count
- Enables bounds checking without type knowledge
- Future `.length()` method can use this information
- Byte size would require division for `.length()`, extra complexity

### 3. Clamp on Out-of-Bounds (v1)

**Decision:** Clamp index to valid range on OOB access for v1.

**Rationale:**
- Safety requirement: prevents memory corruption on embedded
- v1 trade-off: silent wrong result vs crash - we choose "keeps running"
- Future: Trap + stack unwinding for error recovery
- Documented limitation: "OOB accesses clamp, may produce incorrect results"

### 4. Separate `array_map` (Not Enum in `local_map`)

**Decision:** Add `array_map: BTreeMap<Handle<LocalVariable>, ArrayInfo>` instead of extending `local_map` values to an enum.

**Rationale:**
- Keeps non-array path simple (no enum matching overhead)
- Array handling is fundamentally different (pointer-based vs value-based)
- Clear separation of concerns

### 5. Single Flat Slot for Multi-Dimensional Arrays

**Decision:** Store multi-dimensional arrays as a single flat slot (row-major), not nested slots.

**Rationale:**
- Matches C/GLSL semantics (row-major)
- Simpler allocation (one `alloc_slot` call)
- Index computation at lowering time is straightforward
- Better cache locality

## Open Questions

1. **Whole-array operations:** Should we support `arr1 = arr2` (copy) and `arr1 == arr2` (comparison)? GLSL doesn't allow array assignment/comparison, but future versions might.

2. **Unsized array parameters:** GLSL allows `int arr[]` as function parameter (size inferred at call site). Do we need this for v1 or can it wait?

3. **`.length()` method:** Should we add this in v2? It would use the fat pointer length field.

4. **Zero-initialization cost:** Large arrays zero-initialized on declaration could be expensive. Should we have a "lazy zero" mode or require explicit initialization?

## Related Documents

- `docs/roadmaps/2026-03-29-lpir-parity/milestone-iv-array-lowering.md` - Implementation milestone
- `docs/plans/2026-03-30-arrays-notes.md` - Working notes and analysis
- `lp-glsl/lp-glsl-compiler/src/frontend/codegen/stmt/declaration.rs` - Old compiler array declaration
- `lp-glsl/lp-glsl-compiler/src/frontend/semantic/types.rs` - Old compiler type system

## Changelog

| Date | Change |
|------|--------|
| 2026-03-30 | Initial design document - stack slots, fat pointers, bounds clamping |
