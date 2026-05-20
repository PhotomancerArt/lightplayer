# Phase 2: Split Hot Typing Helpers

## Scope Of Phase

Reduce the biggest known stack frames before the arena migration.

In scope:

- Split `type_builtin_args` into smaller helper functions.
- Split `type_glsl_import_args` into smaller helper functions.
- Replace hot `clone`/`collect` patterns with explicit small-arity handling where that improves stack/heap shape.
- Keep the existing recursive `HirExpr` tree representation for this phase.

Out of scope:

- Introducing `ExprId`.
- Introducing `PlaceId`.
- Changing shader semantics or builtin coverage.
- Moving lowering to arena-backed HIR.

## Code Organization Reminders

- Keep builtin typing in `lp-shader/lps-glsl/src/hir/builtin.rs` unless the file becomes clearer with a small sibling module.
- Prefer search-friendly helper names such as `type_unary_float_builtin`, `type_integer_builtin`, or `type_relational_builtin`.
- Keep tests at the bottom of files.
- Avoid helper abstractions that obscure GLSL signature rules.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Update:

- `lp-shader/lps-glsl/src/hir/builtin.rs`
- `lp-shader/lps-glsl/src/hir/coerce.rs` if needed for small helper improvements.

Current concern:

- `type_builtin_args` is about a 2320-byte RV32 stack frame.
- `type_glsl_import_args` is about a 1008-byte RV32 stack frame.
- Both operate on `Vec<HirExpr>` where each `HirExpr` is about 120 bytes.
- Many cases clone `args[i]` and return fresh `alloc::vec![...]`.

Expected shape:

```rust
pub(super) fn type_builtin_args(
    span: Span,
    kind: BuiltinKind,
    args: Vec<HirExpr>,
) -> Result<(Vec<HirExpr>, LpsType), Diagnostic> {
    check_builtin_arity(span, kind, args.len())?;
    match kind {
        BuiltinKind::Abs | BuiltinKind::Floor | BuiltinKind::Fract => {
            type_passthrough_numeric_builtin(span, kind, args)
        }
        BuiltinKind::Equal | BuiltinKind::NotEqual | ... => {
            type_relational_builtin(span, kind, args)
        }
        ...
    }
}
```

Use small arity extraction helpers to consume the vector instead of cloning from it when possible:

```rust
fn one_arg(span: Span, args: Vec<HirExpr>) -> Result<HirExpr, Diagnostic>;
fn two_args(span: Span, args: Vec<HirExpr>) -> Result<(HirExpr, HirExpr), Diagnostic>;
fn three_args(span: Span, args: Vec<HirExpr>) -> Result<(HirExpr, HirExpr, HirExpr), Diagnostic>;
```

The arity check can remain centralized. The extraction helpers should still produce good diagnostics if called incorrectly.

Review these clone-heavy cases:

- `pow`
- two-arg `atan`
- `ldexp`
- `bitfieldInsert`
- `distance`
- `dot`
- `max`
- `min`
- `mod`
- vector comparisons
- `clamp`
- `smoothstep`
- `fma`
- `mix`

Review `coerce_constructor_args`:

- Replace iterator `collect()` with explicit loops if that improves generated stack shape.
- Preserve exact coercion behavior.

After refactoring, rerun the stack measurement from phase 1 and update `measurements.md`.

Expected outcome:

- Lower stack size for `type_builtin_args`.
- Lower or unchanged stack size for `type_glsl_import_args`.
- No behavior changes in GLSL builtin tests.

## Validate

```bash
cargo fmt --check
cargo test -p lps-glsl
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
```

If hardware is attached:

```bash
just demo-esp32c6-check basic
```
