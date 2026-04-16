# Plan A: NumericStrategy + FloatStrategy

Part of the direct-Q32 design (docs/designs/2026-03-11-direct-q32).

## Goal

Introduce the `NumericStrategy` abstraction and `FloatStrategy` implementation
without changing any compiler output. After this plan, every float operation in
the codegen routes through the strategy, but the strategy emits exactly the same
CLIF instructions as the hardcoded calls do today.

This is a pure structural refactor. Zero behavioral change. The test suite must
pass identically.

## Scope

- New file: `frontend/codegen/numeric.rs` — trait, enum, FloatStrategy
- Modified: `frontend/codegen/context.rs` — add `numeric` field
- Modified: ~22 codegen files — replace `builder.ins().fadd(...)` etc. with
  strategy calls
- Modified: `frontend/codegen/signature.rs` — accept scalar type parameter
- Modified: `frontend/glsl_compiler.rs` — pass FloatStrategy when constructing
  CodegenContext
- Modified: `frontend/semantic/types.rs` — `to_cranelift_type` accepts optional
  scalar type override

## Non-scope

- Q32Strategy implementation (Plan B)
- Builtin/libcall dispatch changes (Plan C)
- Pipeline changes (Plan D)

The existing Q32 transform continues to work unchanged. This plan only adds the
abstraction layer.

## Phases

1. Define NumericStrategy trait and FloatStrategy
2. Add numeric field to CodegenContext
3. Update scalar arithmetic call sites (expr/binary, expr/unary, expr/incdec)
4. Update constant emission call sites (expr/literal, expr/variable, builtins, etc.)
5. Update comparison call sites (expr/binary, builtins/relational)
6. Update math/rounding call sites (builtins/common, builtins/geometric)
7. Update composed operations (expr/matrix, builtins/matrix, builtins/interpolation, builtins/trigonometric)
8. Update type references (types::F32 in lvalue, constructor, signature)
9. Update conversion call sites (expr/coercion)
10. Validate
