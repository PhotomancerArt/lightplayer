# Phase 1: ConstValue and const_eval module

## Scope of phase

Add `ConstValue` enum and `eval_constant_expr` for evaluating constant expressions. Supports literals, variable references, binary ops (+, -, *, /, %), unary minus, and type constructors.

## Code Organization Reminders

- One concept per file
- Place more abstract things, entry points first
- Helper functions at bottom

## Implementation Details

1. **const_eval.rs** (NEW):
   - `ConstValue` enum: Int, UInt, Float, Bool, Vec2/3/4, IVec2/3/4, Mat2
   - `ConstValue::glsl_type()` and `to_array_size()`
   - `eval_constant_expr(expr, const_env, span)` for literals, variable refs, unary, binary, FunCall (constructors)
   - Use `glsl::syntax::UnaryOp` and `BinaryOp`

2. **semantic/mod.rs** (UPDATE): Add `pub mod const_eval;`

## Validate

```
cargo build -p lp-glsl-compiler
just filetest const/qualifier/
```
