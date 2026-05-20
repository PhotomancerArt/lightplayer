# Phase 3: Arena-Backed Expressions

## Scope Of Phase

Move typed expressions from recursive owned values to arena IDs.

In scope:

- Add `ExprId`, `ExprList`, and `HirArena` expression storage.
- Change `TypeCtx::type_expr` and expression helpers to return `ExprId`.
- Change expression children from `Box<HirExpr>` / `Vec<HirExpr>` to `ExprId` / `ExprList`.
- Change `HirStmt` and `HirFunctionBody` so statements refer to `ExprId`.
- Update coercion and constant folding to work through the arena.
- Update lowering to consume `ExprId` from the function body's arena.
- Keep places inline for this phase if that makes the migration smaller. In that case,
  variants like `PlaceRead`, `Assign`, and `IncDec` may still store `HirAssignTarget`
  until phase 4.

Out of scope:

- Moving `HirPlace` to `PlaceId`.
- Compacting swizzle/field metadata beyond what expression IDs require.
- Implementing a non-recursive typechecker.
- Rewriting LPIR or native backend codegen.

## Code Organization Reminders

- Add `lp-shader/lps-glsl/src/hir/arena.rs` for arena concepts.
- Keep expression data types in `lp-shader/lps-glsl/src/hir/types.rs`.
- Keep typechecking logic in `lp-shader/lps-glsl/src/hir/typeck.rs`.
- Keep coercion logic in `lp-shader/lps-glsl/src/hir/coerce.rs`.
- Put tests at the bottom of files.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Add:

- `lp-shader/lps-glsl/src/hir/arena.rs`

Update:

- `lp-shader/lps-glsl/src/hir.rs`
- `lp-shader/lps-glsl/src/hir/types.rs`
- `lp-shader/lps-glsl/src/hir/typeck.rs`
- `lp-shader/lps-glsl/src/hir/builtin.rs`
- `lp-shader/lps-glsl/src/hir/builtin_out.rs`
- `lp-shader/lps-glsl/src/hir/coerce.rs`
- `lp-shader/lps-glsl/src/hir/const_fold.rs`
- `lp-shader/lps-glsl/src/lower.rs`
- `lp-shader/lps-glsl/src/lower/ops/*` as required by `lower_expr` signature changes.

Expected arena API:

```rust
impl HirArena {
    pub fn push_expr(&mut self, span: Span, ty: LpsType, kind: HirExprKind) -> ExprId;
    pub fn expr(&self, id: ExprId) -> &HirExpr;
    pub fn expr_mut(&mut self, id: ExprId) -> &mut HirExpr;
    pub fn expr_ty(&self, id: ExprId) -> &LpsType;
    pub fn expr_span(&self, id: ExprId) -> Span;
    pub fn push_expr_list<I>(&mut self, ids: I) -> ExprList
    where
        I: IntoIterator<Item = ExprId>;
    pub fn expr_list(&self, list: ExprList) -> &[ExprId];
}
```

Use `u32` inside IDs. Validate conversions with checked casts or debug assertions so invalid IDs do not silently wrap on embedded.

Change `HirFunctionBody`:

```rust
pub struct HirFunctionBody {
    pub locals: Vec<HirLocal>,
    pub statements: Vec<HirStmt>,
    pub arena: HirArena,
}
```

Change `TypeCtx`:

```rust
pub(super) struct TypeCtx<'a> {
    ...
    arena: HirArena,
}
```

`type_block` should return the arena with the body. One reasonable shape:

```rust
pub(super) fn type_block(
    mut self,
    statements: &[ParsedStmt],
    return_ty: &LpsType,
) -> Result<HirFunctionBody, Diagnostic>
```

If `TypeCtx` cannot be consumed there because of current call patterns, add a `finish_body` helper that moves out `locals`, `statements`, and `arena` together.

Change recursive expression APIs:

```rust
fn type_expr(&mut self, expr: &ParsedExpr) -> Result<ExprId, Diagnostic>;
fn type_binary_values(&mut self, span: Span, op: BinaryOp, lhs: ExprId, rhs: ExprId)
    -> Result<ExprId, Diagnostic>;
fn type_conditional(...) -> Result<ExprId, Diagnostic>;
fn type_expr_args(&mut self, args: &[ParsedExpr]) -> Result<ExprList, Diagnostic>;
```

Change coercion:

```rust
fn coerce_expr(ctx: &mut TypeCtx<'_>, expr: ExprId, target: &LpsType)
    -> Result<ExprId, Diagnostic>;
```

If keeping free functions in `coerce.rs` is cleaner, pass a small arena trait/helper rather than moving all coercion code into `typeck.rs`.

Change constant folding:

- Avoid cloning full expression trees for fold checks.
- Inspect `arena.expr(id).kind`.
- Return `Option<ExprId>` only after allocating the folded expression in the arena, or return a small `FoldedLiteral` enum that typeck inserts.

Change lowering:

```rust
fn lower_expr(
    ctx: &mut LowerCtx<'_>,
    arena: &HirArena,
    expr: ExprId,
) -> Result<LowerValue, Diagnostic>
```

Every old `lower_expr(ctx, &expr)` call should become `lower_expr(ctx, arena, expr_id)`.

Important constraint:

- Do not permanently rebuild an owned recursive `HirExpr` tree before lowering. That would hide the stack problem while preserving or worsening peak heap pressure.

Expected outcome:

- Recursive typechecking returns `ExprId` instead of 120-byte `HirExpr`.
- Statement bodies and expression children no longer store nested `Box<HirExpr>`.
- Most expression-child heap allocations become contiguous arena/list vectors.
- Inline assign targets are the only remaining large expression payloads, and phase 4
  removes those.

## Validate

```bash
cargo fmt --check
cargo test -p lps-glsl
cargo test -p fw-tests --test scene_render_emu --test profile_alloc_emu
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu
```

If hardware is attached:

```bash
just demo-esp32c6-check basic
```

Update `measurements.md` with:

- New top stack frames.
- Device memory lines from the latest trace.
- Any compile-time or code-size change worth noting.
