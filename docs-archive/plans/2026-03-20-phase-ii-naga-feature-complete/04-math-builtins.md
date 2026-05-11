# Phase 4: Math builtins (Expression::Math) + import section

## Scope

Implement `Expression::Math { fun, arg, arg1, arg2, arg3 }` for all
`MathFunction` variants used by the filetests and rainbow.glsl. Build the
WASM import section for external builtin calls.

This unblocks ~30 `builtins/` test files.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### 1. Create imports.rs — import section builder

Before emitting function bodies, scan all functions to discover which builtin
imports are needed:

```rust
pub struct ImportTable {
    /// Import name → WASM function index (imports occupy indices 0..N-1).
    pub map: BTreeMap<String, u32>,
    /// Total number of import functions.
    pub count: u32,
    /// Whether env.memory is needed (true if any imports exist).
    pub needs_memory: bool,
}

impl ImportTable {
    pub fn build(module: &Module, mode: FloatMode) -> Self { ... }
    pub fn emit_import_section(&self, out: &mut ImportSection, types: &mut TypeSection) { ... }
}
```

Scan logic:

- Walk every function's `expressions` arena for `Expression::Math { fun, .. }`
- For each `fun`, determine if it requires an import (see below)
- Assign sequential indices

### 2. Which MathFunctions are inline vs import?

**Float mode — WASM native instructions:**

- `Abs` → `f32.abs`
- `Ceil` → `f32.ceil`
- `Floor` → `f32.floor`
- `Trunc` → `f32.trunc`
- `Round` → `f32.nearest` (rounds to even, matches `roundEven`)
- `Sqrt` → `f32.sqrt`
- `Min` → `f32.min`
- `Max` → `f32.max`
- `Fma` → emit as `a*b + c` (no native fma in WASM)

**Float mode — import required:**

- All trig: `Sin`, `Cos`, `Tan`, `Asin`, `Acos`, `Atan`, `Atan2`
- Hyperbolic: `Sinh`, `Cosh`, `Tanh`, `Asinh`, `Acosh`, `Atanh`
- `Exp`, `Exp2`, `Log`, `Log2`, `Pow`
- `InverseSqrt` → could inline as `1.0/sqrt(x)` or import
- `Mix`, `Clamp`, `SmoothStep`, `Step` → can inline
- `Fract` → inline: `x - floor(x)`
- `Sign` → inline
- `Radians`, `Degrees` → inline: multiply by constant
- `Dot` → inline: sum of products
- `Length` → inline: `sqrt(dot(v,v))`
- `Normalize` → inline: `v / length(v)`
- `Distance` → inline: `length(a - b)`
- `Cross` → inline
- `Reflect`, `Refract`, `FaceForward` → inline

**Q32 mode — everything imports** (the `lps-builtins-wasm` crate provides
Q32 implementations):

- All math functions → import `__lp_<name>`

### 3. Import naming convention

The `lps-builtins-wasm` crate exports functions with `__lp_` prefix.
Map `MathFunction` variant to import name:

```rust
fn math_function_import_name(fun: MathFunction) -> &'static str {
    match fun {
        MathFunction::Floor => "__lp_floor",
        MathFunction::Sin => "__lp_sin",
        MathFunction::Cos => "__lp_cos",
        MathFunction::Clamp => "__lp_clamp",
        MathFunction::Mix => "__lp_mix",
        MathFunction::SmoothStep => "__lp_smoothstep",
        // ... etc
    }
}
```

Check against actual exports from `lps-builtins-wasm` (the `__lp_*`
strings already exist — 58 of them per the build output).

### 4. Create emit_math.rs

```rust
pub fn emit_math(
    fun: MathFunction,
    arg: Handle<Expression>,
    arg1: Option<Handle<Expression>>,
    arg2: Option<Handle<Expression>>,
    arg3: Option<Handle<Expression>>,
    module: &Module,
    func: &Function,
    wasm_fn: &mut WasmFunction,
    mode: FloatMode,
    alloc: &LocalAlloc,
    imports: &ImportTable,
) -> Result<(), String> {
    let dim = expr_value_count(module, func, arg);

    if dim == 1 {
        return emit_math_scalar(fun, arg, arg1, arg2, arg3,
            module, func, wasm_fn, mode, alloc, imports);
    }

    // Vector: apply per-component
    // Store all args to scratch, then process component by component
    emit_math_vector(fun, arg, arg1, arg2, arg3, dim,
        module, func, wasm_fn, mode, alloc, imports)
}
```

For scalar inline (Float mode):

```rust
fn emit_math_scalar_inline(...) -> Result<(), String> {
    match fun {
        MathFunction::Abs => { emit_expr(arg); wasm_fn.instruction(&Instruction::F32Abs); }
        MathFunction::Floor => { emit_expr(arg); wasm_fn.instruction(&Instruction::F32Floor); }
        MathFunction::Ceil => { emit_expr(arg); wasm_fn.instruction(&Instruction::F32Ceil); }
        MathFunction::Sqrt => { emit_expr(arg); wasm_fn.instruction(&Instruction::F32Sqrt); }
        MathFunction::Min => { emit_expr(arg); emit_expr(arg1); wasm_fn.instruction(&Instruction::F32Min); }
        MathFunction::Max => { emit_expr(arg); emit_expr(arg1); wasm_fn.instruction(&Instruction::F32Max); }
        MathFunction::Fract => {
            // x - floor(x)
            emit_expr(arg);
            let scratch = alloc.get_scratch();
            wasm_fn.instruction(&Instruction::LocalTee(scratch));
            wasm_fn.instruction(&Instruction::F32Floor);
            wasm_fn.instruction(&Instruction::LocalGet(scratch));
            wasm_fn.instruction(&Instruction::F32Sub);
        }
        MathFunction::Clamp => {
            // clamp(x, lo, hi) = max(lo, min(x, hi))
            emit_expr(arg);
            emit_expr(arg2.unwrap()); // hi
            wasm_fn.instruction(&Instruction::F32Min);
            emit_expr(arg1.unwrap()); // lo
            wasm_fn.instruction(&Instruction::F32Max);
        }
        MathFunction::Mix => {
            // mix(a, b, t) = a*(1-t) + b*t
            // Or: a + (b - a) * t
            // Emit as import for simplicity, or inline
        }
        // ... trig/exp → import
        _ => emit_math_scalar_import(fun, arg, arg1, arg2, arg3, ...)
    }
}
```

For Q32 mode — always emit as import:

```rust
fn emit_math_scalar_import(...) -> Result<(), String> {
    let import_name = math_function_import_name(fun);
    let import_idx = imports.map.get(import_name)
        .ok_or_else(|| format!("missing import {import_name}"))?;
    emit_expr(arg);
    if let Some(a1) = arg1 { emit_expr(a1); }
    if let Some(a2) = arg2 { emit_expr(a2); }
    if let Some(a3) = arg3 { emit_expr(a3); }
    wasm_fn.instruction(&Instruction::Call(*import_idx));
    Ok(())
}
```

### 5. Vector math

For `Expression::Math` on vectors, most functions are component-wise:

```rust
fn emit_math_vector(..., dim: u32, ...) -> Result<(), String> {
    // Store vector args to scratch
    // For each component k in 0..dim:
    //   push arg[k], arg1[k] if exists, arg2[k] if exists
    //   emit scalar math op or call import
}
```

Some functions are special:

- `Dot(a, b)` → sum of a[i]\*b[i] for i in 0..dim → **returns scalar**
- `Length(v)` → sqrt(dot(v,v)) → **returns scalar**
- `Normalize(v)` → v / length(v) → **returns vector**
- `Cross(a, b)` → only for vec3, returns vec3
- `Distance(a, b)` → length(a - b) → **returns scalar**

These need special-case handling rather than component-wise emission.

### 6. Update emit_module to build import section

In `emit_module()`:

```rust
let imports = ImportTable::build(&naga_module.module, mode);
let import_offset = imports.count;

// Adjust all user function indices by import_offset
let func_index_map: BTreeMap<Handle<Function>, u32> = naga_module
    .functions.iter().enumerate()
    .map(|(i, (h, _))| (*h, i as u32 + import_offset))
    .collect();

// Emit import section before function section
if imports.count > 0 {
    let mut import_sec = ImportSection::new();
    imports.emit(&mut import_sec, &mut types_sec);
    out.section(&import_sec);
}
```

### 7. Relational functions

`Expression::Relational { fun, argument }`:

```rust
RelationalFunction::IsNan => {
    emit_expr(argument);
    // f32: x != x
    wasm_fn.instruction(&Instruction::LocalTee(scratch));
    wasm_fn.instruction(&Instruction::LocalGet(scratch));
    wasm_fn.instruction(&Instruction::F32Ne);
}
RelationalFunction::IsInf => {
    emit_expr(argument);
    wasm_fn.instruction(&Instruction::F32Abs);
    wasm_fn.instruction(&Instruction::F32Const(Ieee32::from(f32::INFINITY)));
    wasm_fn.instruction(&Instruction::F32Eq);
}
RelationalFunction::All | RelationalFunction::Any => {
    // For bvec: All = and all components, Any = or all components
}
```

## Validate

```bash
scripts/filetests.sh --target wasm.q32 "builtins/"
scripts/filetests.sh --target wasm.q32 "scalar/"
scripts/filetests.sh --target wasm.q32 "vec/"
cargo check -p lps-wasm
```

All `builtins/` tests should pass. Some edge cases (NaN/Inf handling differences
between Q32 backends) may need tolerance adjustments.
