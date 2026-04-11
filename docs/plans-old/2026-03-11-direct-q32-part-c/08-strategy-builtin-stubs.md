# Phase 8: Fill in Q32Strategy Builtin Stubs

## Context

Plan B left `todo!()` stubs for operations that require builtin calls:

- `emit_add` (saturating) → `__lp_q32_add`
- `emit_sub` (saturating) → `__lp_q32_sub`
- `emit_mul` (saturating) → `__lp_q32_mul`
- `emit_div` (saturating) → `__lp_q32_div`
- `emit_sqrt` → `__lp_q32_sqrt`

These need module access to call `get_builtin_func_ref`. The question
is how to thread that access into the strategy.

## Approach: Widen the method signatures

The cleanest approach is to give the strategy methods optional access
to the module when they need it. Two options:

### Option 1: Pass GlModule to methods that need it

```rust
pub fn emit_add(
    &self,
    a: Value,
    b: Value,
    builder: &mut FunctionBuilder,
    module: Option<&mut GlModule<M>>,
) -> Value
```

Problem: this makes `NumericMode::emit_add` generic over `M: Module`,
which propagates generics everywhere.

### Option 2: Store a pre-resolved FuncRef table in Q32Strategy

```rust
pub struct Q32Strategy {
    pub opts: Q32Options,
    pub builtins: Q32BuiltinRefs,
}

pub struct Q32BuiltinRefs {
    pub add: Option<FuncRef>,
    pub sub: Option<FuncRef>,
    pub mul: Option<FuncRef>,
    pub div: Option<FuncRef>,
    pub sqrt: Option<FuncRef>,
}
```

Before codegen starts for a function, resolve the builtin FuncRefs
and store them in Q32Strategy. The strategy methods use the pre-resolved
refs directly — no module access needed.

Problem: FuncRef is per-function, so the table needs to be rebuilt for
each function being compiled.

### Option 3: Resolve on demand in CodegenContext

Don't change the strategy methods. Instead, have `CodegenContext` wrapper
methods that handle the builtin call path:

```rust
impl CodegenContext {
    pub fn emit_float_add(&mut self, a: Value, b: Value) -> Value {
        match &self.numeric {
            NumericMode::Float(s) => s.emit_add(a, b, &mut self.builder),
            NumericMode::Q32(s) => {
                if s.opts.add_sub.is_wrapping() {
                    s.emit_add(a, b, &mut self.builder)
                } else {
                    // Saturating: call builtin
                    let func_ref = self.gl_module
                        .get_builtin_func_ref(BuiltinId::LpQ32Add, self.builder.func)
                        .expect("Q32 add builtin");
                    let call = self.builder.ins().call(func_ref, &[a, b]);
                    self.builder.inst_results(call)[0]
                }
            }
        }
    }
}
```

This keeps the strategy simple (only inline ops) and handles builtin
calls at the context level where `gl_module` is available.

**Recommendation**: Option 3. It's the least invasive:
- Strategy methods stay simple (no module access, no pre-resolution).
- The branching between inline and builtin happens in the same place
  the codegen already calls these methods (`emit_float_add`, etc.).
- The `CodegenContext` already has both `gl_module` and `numeric`.
- Only the ~5 operations that have builtin variants need this treatment.

## Implementation

Update these `CodegenContext` wrapper methods:

| Method | Wrapping (inline, via strategy) | Saturating (builtin call) |
|--------|-------------------------------|--------------------------|
| `emit_float_add` | `s.emit_add(a, b, builder)` | `call __lp_q32_add` |
| `emit_float_sub` | `s.emit_sub(a, b, builder)` | `call __lp_q32_sub` |
| `emit_float_mul` | `s.emit_mul(a, b, builder)` | `call __lp_q32_mul` |
| `emit_float_div` | `s.emit_div(a, b, builder)` | `call __lp_q32_div` |
| `emit_float_sqrt` | N/A (sqrt is always a call) | `call __lp_q32_sqrt` |

The `FloatStrategy` path remains unchanged (always returns the inline
result from the strategy).

## Q32Strategy changes

With Option 3, the strategy's `emit_add`/`emit_sub`/`emit_mul`/`emit_div`
only need the wrapping/reciprocal (inline) implementations. The `todo!()`
stubs for saturating variants can be replaced with `unreachable!()` since
the context method handles the dispatch before calling the strategy.

`emit_sqrt` on Q32Strategy can be `unreachable!()` (the context always
calls the builtin for Q32 sqrt) or `todo!()` if we want to keep the
option of inline sqrt later.

## Implementation notes

- The borrow checker may require care: `get_builtin_func_ref` takes
  `&mut GlModule` and `&mut Function`, but `self.builder.func` borrows
  the function. The call to `get_builtin_func_ref` needs to happen before
  the `builder.ins().call(...)` line. Extract the FuncRef first, then use
  builder.
- For sqrt, the existing float path in `emit_float_sqrt` already goes
  through the strategy (`builder.ins().sqrt(a)`). The Q32 path just calls
  the builtin instead.
