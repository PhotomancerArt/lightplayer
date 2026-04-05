# Phase 1: Array Declaration & Slot Allocation

## Scope

Enable array variable declarations to allocate LPIR stack slots and track array metadata.

Target: `test_declaration()` and `test_type_stored()` from `1-foundation.glsl` should pass.

## Implementation Details

### 1. Add Array Type Utilities

In `lp-shader/lps-frontend/src/naga_util.rs`:

```rust
/// Check if a TypeInner represents an array
pub(crate) fn is_array_type(inner: &TypeInner) -> bool {
    matches!(inner, TypeInner::Array { .. })
}

/// Get array element type handle
pub(crate) fn array_element_type(module: &Module, array_ty: Handle<Type>) -> Option<Handle<Type>> {
    match &module.types[array_ty].inner {
        TypeInner::Array { base, .. } => Some(*base),
        _ => None,
    }
}

/// Calculate array size (number of elements)
pub(crate) fn array_element_count(inner: &TypeInner) -> Option<u32> {
    match inner {
        TypeInner::Array { size, .. } => Some(size.nice_unwrap()),
        _ => None,
    }
}

/// Calculate total array size in bytes
pub(crate) fn array_size_bytes(module: &Module, array_ty: Handle<Type>) -> Option<u32> {
    let inner = &module.types[array_ty].inner;
    let TypeInner::Array { base, size } = inner else {
        return None;
    };
    
    let elem_inner = &module.types[*base].inner;
    let elem_size = element_size_bytes(elem_inner)?;
    let count = size.nice_unwrap();
    
    Some(elem_size * count)
}

/// Get size of a (non-array) element in bytes
fn element_size_bytes(inner: &TypeInner) -> Option<u32> {
    match inner {
        TypeInner::Scalar(_) => Some(4),
        TypeInner::Vector { size, .. } => Some(4 * vector_size_usize(*size) as u32),
        TypeInner::Matrix { columns, rows, .. } => {
            Some(4 * vector_size_usize(*columns) as u32 * vector_size_usize(*rows) as u32)
        }
        _ => None,
    }
}
```

### 2. Add ArrayInfo struct and array_map

In `lp-shader/lps-frontend/src/lower_ctx.rs`:

```rust
/// Metadata for array-typed local variables
pub(crate) struct ArrayInfo {
    pub slot: SlotId,
    pub element_ty: Handle<Type>,
    pub element_size: u32,  // bytes
    pub element_count: u32,
}

pub(crate) struct LowerCtx<'a> {
    // ... existing fields ...
    pub array_map: BTreeMap<Handle<LocalVariable>, ArrayInfo>,
}
```

### 3. Modify Local Variable Initialization

In `LowerCtx::new()`, after handling non-array locals:

```rust
// Allocate slots for array-typed locals
for (lv_handle, var) in func.local_variables.iter() {
    if param_aliases.contains_key(&lv_handle) || local_map.contains_key(&lv_handle) {
        continue; // Already handled
    }
    
    let ty_inner = &module.types[var.ty].inner;
    if let TypeInner::Array { base, size } = ty_inner {
        let elem_inner = &module.types[*base].inner;
        let elem_size = element_size_bytes(elem_inner)
            .ok_or_else(|| LowerError::UnsupportedType("array element type".into()))?;
        let count = size.nice_unwrap();
        let total_size = elem_size * count;
        
        let slot = fb.alloc_slot(total_size);
        
        array_map.insert(lv_handle, ArrayInfo {
            slot,
            element_ty: *base,
            element_size: elem_size,
            element_count: count,
        });
    }
}
```

### 4. Helper method for array lookup

```rust
impl<'a> LowerCtx<'a> {
    pub(crate) fn resolve_array(&self, lv: Handle<LocalVariable>) -> Result<&ArrayInfo, LowerError> {
        self.array_map
            .get(&lv)
            .ok_or_else(|| LowerError::Internal(format!("unknown array variable {lv:?}")))
    }
}
```

## Tests to Verify

From `array/phase/1-foundation.glsl`:

```glsl
int test_declaration() {
    int arr[5];
    return 0; // Just verify declaration compiles
}
// run: test_declaration() == 0

int test_type_stored() {
    int arr[5];
    // This should work if array type is stored correctly
    return 0;
}
// run: test_type_stored() == 0
```

## Validation

```bash
scripts/glsl-filetests.sh array/phase/1-foundation.glsl
```

Expected: First 2 tests pass (declaration and type storage).

## Code Quality Notes

- Keep `array_map` separate from `local_map` - different access patterns
- Place helper utilities at bottom of files
- Use `nice_unwrap()` for naga array sizes (handles constant expressions)
- Add TODO comments for multi-dimensional array support (deferred)
