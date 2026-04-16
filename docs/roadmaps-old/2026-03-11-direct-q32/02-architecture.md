# Architecture: NumericStrategy Trait

## Current flow

```
GLSL source
  → parse + semantic analysis (TypedShader, float semantics)
  → codegen (float CLIF IR, hardcoded fadd/fmul/f32const/...)
  → Q32 transform (rewrite entire IR: F32→I32, fadd→q32_add, ...)
  → compile (machine code)
```

## Proposed flow

```
GLSL source
  → parse + semantic analysis (TypedShader, float semantics — unchanged)
  → codegen (CLIF IR via NumericStrategy — emits Q32 directly if configured)
  → compile (machine code)
```

The semantic analysis layer is untouched. It continues to reason about floats,
vectors, matrices. The `TypedShader` remains a float-semantic AST.

The change is in codegen: instead of hardcoding `builder.ins().fadd(a, b)`,
the codegen calls `strategy.emit_add(a, b, builder)`, and the strategy
decides what instructions to emit.

## The trait

```rust
/// Pluggable numeric representation for the codegen layer.
///
/// The GLSL semantic analysis always works with float semantics.
/// This trait controls how those semantics map to CLIF IR instructions.
pub trait NumericStrategy {
    /// The CLIF type used for GLSL `float` values.
    fn scalar_type(&self) -> Type;

    // --- Constants ---

    fn emit_const(&self, val: f32, pos: &mut FuncCursor) -> Value;

    // --- Arithmetic ---

    fn emit_add(&self, a: Value, b: Value, pos: &mut FuncCursor) -> Value;
    fn emit_sub(&self, a: Value, b: Value, pos: &mut FuncCursor) -> Value;
    fn emit_mul(&self, a: Value, b: Value, pos: &mut FuncCursor) -> Value;
    fn emit_div(&self, a: Value, b: Value, pos: &mut FuncCursor) -> Value;
    fn emit_neg(&self, a: Value, pos: &mut FuncCursor) -> Value;
    fn emit_abs(&self, a: Value, pos: &mut FuncCursor) -> Value;

    // --- Comparison ---

    /// Emit a comparison. `cc` uses FloatCC semantics (Equal, LessThan, etc.)
    /// even when the underlying type is integer. The strategy translates to
    /// the appropriate integer comparison when needed.
    fn emit_cmp(&self, cc: FloatCC, a: Value, b: Value, pos: &mut FuncCursor) -> Value;
    fn emit_min(&self, a: Value, b: Value, pos: &mut FuncCursor) -> Value;
    fn emit_max(&self, a: Value, b: Value, pos: &mut FuncCursor) -> Value;

    // --- Rounding ---

    fn emit_floor(&self, a: Value, pos: &mut FuncCursor) -> Value;
    fn emit_ceil(&self, a: Value, pos: &mut FuncCursor) -> Value;
    fn emit_trunc(&self, a: Value, pos: &mut FuncCursor) -> Value;
    fn emit_nearest(&self, a: Value, pos: &mut FuncCursor) -> Value;

    // --- Math ---

    fn emit_sqrt(&self, a: Value, pos: &mut FuncCursor) -> Value;

    // --- Conversions ---

    /// Convert an integer to the numeric scalar type.
    fn emit_from_int(&self, a: Value, pos: &mut FuncCursor) -> Value;
    /// Convert the numeric scalar type to an integer.
    fn emit_to_int(&self, a: Value, pos: &mut FuncCursor) -> Value;

    // --- Signatures ---

    /// Transform a float-semantic Signature to the target representation.
    /// For FloatStrategy this is identity. For Q32Strategy, F32 → I32.
    fn map_signature(&self, sig: &Signature) -> Signature;
}
```

## Cursor vs Builder

The trait methods take `&mut FuncCursor` (or similar insertion-point
abstraction) rather than `&mut FunctionBuilder` to keep them focused on
instruction emission. The builder owns the cursor; call sites can obtain a
cursor from it. If this is impractical, the methods can take
`&mut FunctionBuilder` directly — the trait shouldn't dictate the builder
API, but it should stay minimal.

In practice, Q32 operations like multiply need to emit multiple instructions
(e.g. `imul` + `sshr` for fixed-point multiply), so the emission point needs
to support multi-instruction sequences. Both FunctionBuilder and FuncCursor
support this.

## FloatStrategy

Trivial implementation — each method maps directly to the corresponding
CLIF instruction:

```rust
impl NumericStrategy for FloatStrategy {
    fn scalar_type(&self) -> Type { types::F32 }
    fn emit_const(&self, val: f32, pos: &mut FuncCursor) -> Value {
        pos.ins().f32const(val)
    }
    fn emit_add(&self, a: Value, b: Value, pos: &mut FuncCursor) -> Value {
        pos.ins().fadd(a, b)
    }
    // ... etc
    fn map_signature(&self, sig: &Signature) -> Signature {
        sig.clone()
    }
}
```

This produces exactly what the compiler produces today. It's the
"make it work, don't break anything" baseline.

## Q32Strategy

Each method emits the Q16.16 fixed-point equivalent. The implementations
can be extracted directly from the existing Q32 transform code
(`backend/transform/q32/instructions.rs`), which already has the logic for
each operation:

```rust
impl NumericStrategy for Q32Strategy {
    fn scalar_type(&self) -> Type { types::I32 }

    fn emit_const(&self, val: f32, pos: &mut FuncCursor) -> Value {
        let q32_val = (val * 65536.0) as i32;
        pos.ins().iconst(types::I32, q32_val as i64)
    }

    fn emit_add(&self, a: Value, b: Value, pos: &mut FuncCursor) -> Value {
        // Saturating or wrapping, controlled by Q32Options
        pos.ins().iadd(a, b)  // or iadd_sat for saturating
    }

    fn emit_mul(&self, a: Value, b: Value, pos: &mut FuncCursor) -> Value {
        // Q16.16 multiply: (a * b) >> 16
        // Widen to 64-bit to avoid overflow:
        let a_ext = pos.ins().sextend(types::I64, a);
        let b_ext = pos.ins().sextend(types::I64, b);
        let product = pos.ins().imul(a_ext, b_ext);
        let shifted = pos.ins().sshr_imm(product, 16);
        pos.ins().ireduce(types::I32, shifted)
    }
    // ... etc
}
```

## What the trait does NOT cover

### Builtin/library function calls

The trait handles inline operations. For transcendental functions (sin, cos,
pow, etc.), the codegen already has a separate dispatch path:

- `emit_builtin_call` in `builtins/mod.rs`
- `get_math_libcall` / `get_math_libcall_2arg` in `builtins/helpers.rs`
- LPFX functions via `emit_lp_lib_fn_call` in `lpfx_fns.rs`

These paths would become numeric-format-aware through a separate mechanism
(see 03-builtin-dispatch.md), not through the trait. The trait stays focused
on operations the compiler can emit inline.

### Vector/matrix operations

These are composed from scalar operations in `expr/vector.rs` and
`expr/matrix.rs`. They loop over components, calling the scalar binary/unary
operations. Since those scalar operations would go through the strategy,
vector/matrix operations get Q32 support for free.

### Control flow, variables, memory

Unaffected. The strategy only touches numeric values. Control flow, variable
management, stack slots, and memory operations remain unchanged.

## Integration with CodegenContext

```rust
pub struct CodegenContext<'a, M: Module> {
    pub builder: FunctionBuilder<'a>,
    pub gl_module: &'a mut GlModule<M>,
    pub numeric: &'a dyn NumericStrategy,  // NEW
    // ... rest unchanged
}
```

Or, if we want to avoid the dynamic dispatch overhead (trait objects have
vtable indirection), use a generic parameter:

```rust
pub struct CodegenContext<'a, M: Module, N: NumericStrategy> {
    pub builder: FunctionBuilder<'a>,
    pub gl_module: &'a mut GlModule<M>,
    pub numeric: &'a N,
    // ...
}
```

The generic parameter propagates through the codegen functions. Since there
are only two strategies and they're selected at compile time (via the
DecimalFormat option), this is fine. If the propagation is too noisy, a
trait object (`&dyn NumericStrategy`) is acceptable — the vtable cost is
negligible compared to the instruction emission cost.

A third option: make CodegenContext non-generic over N, and instead store
an enum:

```rust
enum NumericMode {
    Float(FloatStrategy),
    Q32(Q32Strategy),
}
```

This avoids both generics propagation and dynamic dispatch (the enum is
stack-allocated and branch-predicted). The methods would dispatch via match.

Recommended: start with the enum approach. It's the least disruptive to
existing code and avoids generic parameter noise. If a third strategy is
ever added, refactor to a trait at that point.
