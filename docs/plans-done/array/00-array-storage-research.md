# Array Storage Research - Cranelift Memory Allocation

## Key Finding: Use Stack Allocation with Pointers

Based on research of Cranelift examples (`cranelift-examples/examples/struct-layouts/main.rs`), arrays should be stored as **stack-allocated memory blocks** accessed via **pointers**, not as individual variables.

## Pattern from Examples

### 1. Allocate Stack Slot

```rust
use cranelift_codegen::ir::StackSlot;

let size = array_size * element_size_bytes;
let stack_slot = builder.create_sized_stack_slot(
    StackSlotData::new(
        StackSlotKind::ExplicitSlot,
        size,
        0  // alignment offset
    )
);
```

### 2. Get Pointer to Stack Slot

```rust
let pointer_type = isa.pointer_type();
let array_ptr = builder.ins().stack_addr(pointer_type, stack_slot, 0);
```

### 3. Store to Array Element

```rust
let offset = index * element_size_bytes;
let flags = MemFlags::trusted();  // or appropriate flags
builder.ins().store(flags, value, array_ptr, offset);
```

### 4. Load from Array Element

```rust
let offset = index * element_size_bytes;
let flags = MemFlags::trusted();
let value = builder.ins().load(element_type, flags, array_ptr, offset);
```

## Implementation Strategy

### Variable Declaration

Instead of:

```rust
// OLD: Multiple variables
let vars = Vec::new();
for _ in 0..total_elements {
    vars.push(builder.declare_var(element_type));
}
```

Do:

```rust
// NEW: Single stack slot + pointer
let array_size_bytes = array_size * element_size_bytes;
let stack_slot = builder.create_sized_stack_slot(...);
let array_ptr = builder.ins().stack_addr(pointer_type, stack_slot, 0);
// Store pointer in variable table (not individual vars)
```

### Variable Storage

- Store `array_ptr` (pointer Value) instead of `Vec<Variable>`
- Need to track: `(stack_slot, array_ptr, element_type, element_size_bytes, array_size)`

### Array Indexing

- Calculate offset: `offset = index * element_size_bytes`
- Use `load`/`store` with pointer + offset
- For multi-dimensional: `offset = i0 * size1 * size2 * ... + i1 * size2 * ... + i2 * ...`

### Bounds Checking

- Check: `index < 0 || index >= array_size`
- Use `icmp` and `trapnz` before load/store

## Changes Needed

### 1. `VarInfo` Structure

```rust
pub struct VarInfo {
    // For arrays: store pointer instead of vars
    array_ptr: Option<Value>,  // Pointer to stack-allocated array
    stack_slot: Option<StackSlot>,  // Stack slot for array
    element_type: GlslType,
    element_size_bytes: usize,
    array_size: usize,

    // For non-arrays: existing fields
    cranelift_vars: Vec<Variable>,
    glsl_type: GlslType,
}
```

### 2. `declare_variable()` Changes

- Detect array type
- Allocate stack slot
- Get pointer
- Store pointer in VarInfo

### 3. `LValue::ArrayElement` Changes

- Store `array_ptr: Value` instead of `base_vars: Vec<Variable>`
- Calculate offset dynamically
- Use `load`/`store` with pointer + offset

### 4. Element Size Calculation

- Scalar: `element_type.bytes()` (e.g., `I32` = 4 bytes)
- Vector: `component_count * base_type.bytes()` (e.g., `vec4` = 4 \* 4 = 16 bytes)
- Matrix: `rows * cols * 4` (always float = 4 bytes)
- Nested array: recursive calculation

## Alignment Considerations

From the examples, alignment is important:

- Each element should be aligned to its type's natural alignment
- Stack slots are automatically aligned by Cranelift
- For element access: `offset` should be element-aligned

## Example: `float arr[5]`

```rust
// Declaration
let element_size = 4; // float = 4 bytes
let array_size = 5;
let total_size = array_size * element_size; // 20 bytes
let stack_slot = builder.create_sized_stack_slot(...);
let array_ptr = builder.ins().stack_addr(pointer_type, stack_slot, 0);

// Access arr[2]
let index = 2;
let offset = index * element_size; // 2 * 4 = 8
let value = builder.ins().load(types::F32, MemFlags::trusted(), array_ptr, offset);
```

## Example: `vec4 arr[3]`

```rust
// Declaration
let element_size = 16; // vec4 = 4 components * 4 bytes = 16 bytes
let array_size = 3;
let total_size = array_size * element_size; // 48 bytes
let stack_slot = builder.create_sized_stack_slot(...);
let array_ptr = builder.ins().stack_addr(pointer_type, stack_slot, 0);

// Access arr[1].x (first component of second element)
let index = 1;
let element_offset = index * element_size; // 1 * 16 = 16
let component_offset = 0; // x component
let total_offset = element_offset + component_offset; // 16
let value = builder.ins().load(types::F32, MemFlags::trusted(), array_ptr, total_offset);
```

## Benefits

1. **Memory efficient**: Single allocation vs many variables
2. **Proper semantics**: Arrays are contiguous memory blocks
3. **Supports large arrays**: Not limited by variable count
4. **Pointer arithmetic**: Natural indexing with offsets
5. **Function parameters**: Can pass arrays as pointers

## Migration Path

1. **Phase 1**: Implement array storage with stack slots
2. **Phase 2**: Migrate existing variable-based storage (if needed)
3. **Phase 3**: Support array function parameters (pass by pointer)

## References

- `cranelift-examples/examples/struct-layouts/main.rs` - Stack allocation pattern
- `cranelift-jit-demo/src/jit.rs` - JIT module usage
- Current codebase: `function.rs` line 270 - `stack_addr` usage for struct returns
