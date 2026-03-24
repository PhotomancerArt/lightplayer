# Phase 2: Vectors — scalarized emission

## Scope

Add vector support to the WASM backend. Vectors are emitted as N flat scalar
values on the WASM stack. Vector locals occupy N consecutive WASM locals.
Multi-value returns emit N values.

This unblocks ~80 test files in `vec/`.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### 1. Update locals.rs — vector local allocation

Currently `LocalAlloc` allocates 1 WASM local per `LocalVariable`. For vectors,
allocate N:

```rust
// In LocalAlloc::new(), when iterating func.local_variables:
let inner = &module.types[lv.ty].inner;
let slots = match *inner {
    TypeInner::Vector { size, .. } => size as u32,
    _ => 1,
};
for _ in 0..slots {
    let vt = scalar_naga_inner_to_valtype(inner, mode);
    extra_local_valtypes.push(vt);
}
local_map.insert(handle, next);
next += slots;
```

Add a helper:

```rust
pub fn local_variable_slots(&self, module: &Module, func: &Function, lv: Handle<LocalVariable>) -> u32 {
    let inner = &module.types[func.local_variables[lv].ty].inner;
    match *inner {
        TypeInner::Vector { size, .. } => size as u32,
        _ => 1,
    }
}
```

### 2. Create emit_vec.rs — vector expression helpers

Core concept: `emit_expr` currently pushes **one** value onto the WASM stack.
For vectors, `emit_expr` must push **N** values. The caller must know how many
values were pushed.

Approach: `emit_expr` always pushes the correct number of values for the
expression's type. A scalar pushes 1. A vec3 pushes 3. The caller looks up
the expression's type to know the count.

Helper function:

```rust
/// Number of WASM stack values produced by emitting this expression.
fn expr_value_count(module: &Module, func: &Function, expr: Handle<Expression>) -> u32 {
    // Resolve type, return dimension for vectors, 1 for scalars
}
```

### 3. Expression::Compose

`Compose { ty, components }` — each component is already emitted as its natural
width (scalar = 1, vec2 = 2, etc.). Just emit all components in order:

```rust
Expression::Compose { ty, components } => {
    for &c in components {
        emit_expr(module, func, c, wasm_fn, mode, alloc)?;
    }
    Ok(())
}
```

Example: `vec3(1.0, 2.0, 3.0)` → components are 3 scalar literals → emits 3 values.
Example: `vec4(v3, 1.0)` → components are a vec3 + scalar → emits 4 values.

### 4. Expression::Splat

`Splat { size, value }` — broadcast a scalar to N components:

```rust
Expression::Splat { size, value } => {
    let dim = *size as u32;
    emit_expr(module, func, *value, wasm_fn, mode, alloc)?;
    if dim > 1 {
        // Store to a temp local, then emit N local.gets
        let scratch = alloc.alloc_temp(mode)?; // or use existing scratch
        wasm_fn.instruction(&Instruction::LocalSet(scratch));
        for _ in 0..dim {
            wasm_fn.instruction(&Instruction::LocalGet(scratch));
        }
    }
    Ok(())
}
```

For Splat, we need one scratch local. The existing `q32_scratch` pair can be
repurposed, or allocate a dedicated "splat scratch" local in `LocalAlloc`.

Better approach: add a small pool of general-purpose scratch locals in
`LocalAlloc` (e.g. 4 i32 + 4 f32). These are used for Splat, Swizzle, and
vector binary shuffling.

### 5. Expression::Swizzle

`Swizzle { size, vector, pattern }` — reorder components of a vector:

```rust
Expression::Swizzle { size, vector, pattern } => {
    let src_dim = expr_value_count(module, func, *vector);
    let dst_dim = *size as u32;

    // Emit source vector to scratch locals
    emit_expr(module, func, *vector, wasm_fn, mode, alloc)?;
    let base_scratch = alloc.alloc_temp_n(src_dim as usize, mode)?;
    // Store in reverse order (stack is LIFO)
    for i in (0..src_dim).rev() {
        wasm_fn.instruction(&Instruction::LocalSet(base_scratch + i));
    }

    // Re-emit in pattern order
    for i in 0..dst_dim {
        let component = pattern[i as usize] as u32;
        wasm_fn.instruction(&Instruction::LocalGet(base_scratch + component));
    }
    Ok(())
}
```

### 6. Expression::AccessIndex

`AccessIndex { base, index }` — extract a single scalar from a vector:

```rust
Expression::AccessIndex { base, index } => {
    let src_dim = expr_value_count(module, func, *base);
    if src_dim == 1 {
        // Scalar access — just emit base
        return emit_expr(module, func, *base, wasm_fn, mode, alloc);
    }

    // Emit full vector to scratch locals
    emit_expr(module, func, *base, wasm_fn, mode, alloc)?;
    let base_scratch = alloc.alloc_temp_n(src_dim as usize, mode)?;
    for i in (0..src_dim).rev() {
        wasm_fn.instruction(&Instruction::LocalSet(base_scratch + i));
    }

    // Get the one component we want
    wasm_fn.instruction(&Instruction::LocalGet(base_scratch + *index));
    Ok(())
}
```

### 7. Vector binary operations

For `Binary { op, left, right }` where both operands are vectors of dimension N:

Emit left (N values), emit right (N values). The stack now has:
`[L0, L1, ..., LN-1, R0, R1, ..., RN-1]`

Need to produce `[L0 op R0, L1 op R1, ..., LN-1 op RN-1]`.

Approach: store both vectors to scratch locals, then emit component-wise:

```rust
// Store right vector
for i in (0..dim).rev() {
    wasm_fn.instruction(&Instruction::LocalSet(right_base + i));
}
// Store left vector
for i in (0..dim).rev() {
    wasm_fn.instruction(&Instruction::LocalSet(left_base + i));
}
// Emit component-wise
for i in 0..dim {
    wasm_fn.instruction(&Instruction::LocalGet(left_base + i));
    wasm_fn.instruction(&Instruction::LocalGet(right_base + i));
    emit_binary_scalar(op, kind, mode, wasm_fn)?;
}
```

This requires 2*N scratch locals. For vec4 that's 8.

### 8. Vector Store and Load

`Statement::Store` for a vector local: emit the vector expression (N values),
then `local.set` each component:

```rust
// After emit_expr pushes N values on stack:
let base = alloc.resolve_local_variable(lv).unwrap();
let dim = alloc.local_variable_slots(module, func, lv);
for i in (0..dim).rev() {
    wasm_fn.instruction(&Instruction::LocalSet(base + i));
}
```

`Expression::Load { pointer: LocalVariable(lv) }` for a vector:

```rust
let base = alloc.resolve_local_variable(lv).unwrap();
let dim = alloc.local_variable_slots(module, func, lv);
for i in 0..dim {
    wasm_fn.instruction(&Instruction::LocalGet(base + i));
}
```

### 9. Vector Return

For `Statement::Return { value: Some(h) }` where `h` is a vector expression,
just emit the expression (pushes N values) then `return`. The function type
already has N results.

### 10. Vector parameter aliases

Currently `param_local_to_argument` maps `LocalVariable` → argument index.
For vector parameters, the WASM function receives N consecutive parameters.
The `FunctionArgument(i)` index `i` is the **argument** index, not the
WASM local index.

Need: map argument index to WASM local base index. For scalar args, arg `i`
maps to local `i`. For vector args, arg `i` maps to local `sum(widths[0..i])`.

Update `LocalAlloc` to build an argument-index-to-wasm-local-base map:

```rust
let mut arg_wasm_base = Vec::new();
let mut wasm_local = 0u32;
for arg in &func.arguments {
    arg_wasm_base.push(wasm_local);
    let slots = type_slot_count(module, arg.ty);
    wasm_local += slots;
}
```

Then `FunctionArgument(i)` emits `local.get(arg_wasm_base[i])` through
`local.get(arg_wasm_base[i] + dim - 1)`.

### 11. Scratch local management

Add a scratch local pool in `LocalAlloc`:

```rust
pub struct LocalAlloc {
    // ... existing fields ...
    scratch_base: u32,
    scratch_count: u32,
}
```

Allocate a fixed pool (e.g. 8 locals of the appropriate type) at the end of
the locals section. Provide `alloc_temp_n(n) -> u32` that returns the base
index. For Phase II, a fixed pool of 8 is sufficient (vec4 binary needs 8).

### 12. emit_expr dispatch update

The main `emit_expr` function needs to check the expression's output type
and dispatch to vector-aware paths:

```rust
fn emit_expr(...) -> Result<(), String> {
    let dim = expr_value_count(module, func, expr);
    if dim > 1 {
        return emit_vec_expr(module, func, expr, wasm_fn, mode, alloc, dim);
    }
    // ... existing scalar path ...
}
```

Or, more cleanly: make each match arm handle both scalar and vector cases.
The Compose/Splat/Swizzle/AccessIndex arms are vector-only. Binary/Unary/Load
need to branch based on dimension.

## Validate

```bash
scripts/glsl-filetests.sh --target wasm.q32 "vec/"
scripts/glsl-filetests.sh --target wasm.q32 "scalar/"
cargo check -p lp-glsl-wasm
```

Scalar tests must remain passing. Most `vec/` tests for basic construction and
arithmetic should pass. Tests requiring builtins (like `fn-min`, `fn-max`) will
still fail until Phase 4.
