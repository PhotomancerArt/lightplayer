# Design: Unified Pointer-Based LValue Abstraction

## Current Architecture

### Storage Models

The compiler currently uses three different storage models:

1. **SSA Variables** (Cranelift `Variable`s)
   - Used for: Regular local variables, function parameters (in)
   - Storage: Cranelift SSA variables
   - Access: Direct variable access via `use_var`/`def_var`

2. **Array Pointers** (`LValue::ArrayElement`)
   - Used for: Array elements
   - Storage: Pointer to stack-allocated memory (`array_ptr: Value`)
   - Access: Load/store with calculated offsets

3. **Out/Inout Parameter Pointers** (`LValue::Variable` / `LValue::Component`)
   - Used for: Out/inout function parameters
   - Storage: Pointer stored in `VarInfo.out_inout_ptr`
   - Access: Runtime lookup via name → VarInfo → pointer

### The Problem

The third model (out/inout) is inconsistent:

- Arrays store pointer directly in LValue variant
- Out/inout store pointer in VarInfo, accessed via name lookup
- This forces verbose runtime checks in read/write functions

## Proposed Design

### Unified Pointer Variant

Create a single variant for all pointer-based storage:

```rust
pub enum LValue {
    /// SSA-based variable: `x`
    Variable {
        vars: Vec<Variable>,
        ty: GlslType,
    },

    /// SSA-based component access: `v.x` or `v.xy`
    Component {
        base_vars: Vec<Variable>,
        base_ty: GlslType,
        indices: Vec<usize>,
        result_ty: GlslType,
    },

    /// Pointer-based storage: arrays, out/inout params, future structs
    PointerBased {
        ptr: Value,
        base_ty: GlslType,
        access_pattern: PointerAccessPattern,
    },

    // ... other variants (MatrixElement, MatrixColumn, VectorElement)
}

/// Describes how to access data from a pointer
#[derive(Debug, Clone)]
pub enum PointerAccessPattern {
    /// Direct access: entire variable/vector/matrix
    /// Examples: `arr` (array variable), `v` (out/inout vec3 param)
    Direct {
        component_count: usize,
    },

    /// Component access: `v.x`, `arr[i].xy`
    /// Examples: `v.x` (out/inout param component), `arr[0].x` (array element component)
    Component {
        indices: Vec<usize>,
        result_ty: GlslType,
    },

    /// Array element access: `arr[i]`
    /// Examples: `arr[0]`, `arr[idx]`
    ArrayElement {
        index: Option<usize>,           // Compile-time constant
        index_val: Option<Value>,       // Runtime index
        element_ty: GlslType,
        element_size_bytes: usize,
        component_indices: Option<Vec<usize>>,  // For arr[i].x
    },
}
```

### Benefits

1. **Type Safety**: Storage model is explicit in the type system
2. **No Runtime Lookups**: Pointer available at LValue creation time
3. **Unified Abstraction**: All pointer-based storage uses same variant
4. **Clear Separation**: SSA vs pointer-based is explicit

### Migration Strategy

#### Step 1: Add New Variant (Non-Breaking)

Add `PointerBased` variant alongside existing variants. Initially, don't use it.

#### Step 2: Update Out/Inout Parameter Resolution

When resolving out/inout parameters, create `PointerBased` instead of `Variable`:

```rust
// Before:
LValue::Variable {
    vars: vec![],
    ty: param.ty.clone(),
    name: Some(param.name.clone()),
}

// After:
LValue::PointerBased {
    ptr: pointer_val,
    base_ty: param.ty.clone(),
    access_pattern: PointerAccessPattern::Direct {
        component_count: param.ty.component_count().unwrap_or(1),
    },
}
```

#### Step 3: Update Component Access on Out/Inout

When accessing components of out/inout params, use `PointerBased` with `Component` pattern:

```rust
// Before: LValue::Component with name lookup
// After:
LValue::PointerBased {
    ptr: base_ptr,
    base_ty: base_ty.clone(),
    access_pattern: PointerAccessPattern::Component {
        indices,
        result_ty,
    },
}
```

#### Step 4: Update Read/Write Functions

Add handling for `PointerBased` variant:

```rust
match lvalue {
    LValue::PointerBased { ptr, base_ty, access_pattern } => {
        match access_pattern {
            PointerAccessPattern::Direct { component_count } => {
                // Load all components
                for i in 0..component_count {
                    let offset = (i * component_size_bytes) as i32;
                    let val = ctx.builder.ins().load(base_cranelift_ty, flags, ptr, offset);
                    vals.push(val);
                }
            }
            PointerAccessPattern::Component { indices, .. } => {
                // Load only requested components
                for &idx in indices {
                    let offset = (idx * component_size_bytes) as i32;
                    let val = ctx.builder.ins().load(base_cranelift_ty, flags, ptr, offset);
                    vals.push(val);
                }
            }
            PointerAccessPattern::ArrayElement { .. } => {
                // Handle array element access
            }
        }
    }
    // ... existing variants
}
```

#### Step 5: Migrate Out/Inout Arrays

Out/inout array parameters should also use `PointerBased` with `Direct` pattern:

```rust
// Out/inout array parameter: `arr` (where arr is out/inout)
LValue::PointerBased {
    ptr: array_ptr,
    base_ty: array_ty,
    access_pattern: PointerAccessPattern::Direct {
        component_count: array_size * element_component_count,
    },
}
```

Note: Regular array element access (`arr[i]`) will continue to use `LValue::ArrayElement` for now, or could be migrated to `PointerBased` with `ArrayElement` pattern in a future refactoring.

#### Step 6: Remove Old Code

- Remove `name` field from `Variable` and `Component` variants
- Remove runtime lookups in read/write functions
- Remove `out_inout_ptr` from `VarInfo` (removed immediately after migration)

## Implementation Details

### Pointer Access Pattern Calculation

For out/inout parameters:

- **Direct**: When accessing entire variable (`v`)
- **Component**: When accessing components (`v.x`, `v.xy`)

For arrays:

- **ArrayElement**: When accessing elements (`arr[i]`)
- **Component** (nested): When accessing components of elements (`arr[i].x`)

### Offset Calculation

All pointer-based access uses the same offset calculation:

- Base type determines component size
- Component index determines offset: `offset = index * component_size_bytes`
- For arrays: `offset = element_index * element_size_bytes + component_offset`

### Type Information

`base_ty` in `PointerBased` stores the full type (vector, matrix, array, scalar):

- Used to determine component count
- Used to determine base Cranelift type
- Used for type checking

## Edge Cases

### Nested Component Access

`arr[i].x` on an array of vectors:

- First resolve to `ArrayElement` with `component_indices`
- Could be represented as `PointerBased` with nested `ArrayElement` + `Component`

### Out/Inout Array Parameters

Arrays as out/inout parameters:

- Currently handled specially (use `array_ptr` from `VarInfo`)
- After migration: use `PointerBased` with `Direct` pattern
- Array element access (`arr[i]`) continues to use `LValue::ArrayElement` (or could be migrated later)

### Component Swizzling

`v.xy` on out/inout parameter:

- `PointerBased` with `Component` pattern, `indices = [0, 1]`
- Load components at offsets 0 and 4 (for float)

## Testing Strategy

1. **Existing Tests**: All current tests should pass without modification
2. **New Tests**: Add tests specifically for `PointerBased` variant
3. **Performance**: Benchmark to verify elimination of lookups improves performance
4. **Edge Cases**: Test nested access patterns, component swizzling, etc.

## Future Considerations

### Struct Support

When structs are added, they will likely be pointer-based:

- Structs as out/inout parameters → `PointerBased` with `Direct` or `Component`
- Struct fields → `PointerBased` with field offset calculation

### Optimization Opportunities

With unified variant:

- Easier to optimize pointer-based access patterns
- Can combine multiple loads/stores into single operations
- Better alias analysis (all pointer-based access in one place)
