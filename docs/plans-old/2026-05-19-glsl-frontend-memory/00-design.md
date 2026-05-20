# GLSL Frontend Memory Refactor Design

## Scope

This design gives the GLSL frontend an embedded-friendly HIR representation.

The core change is:

> typed HIR expressions and places should be arena objects addressed by small IDs, not recursive owned values passed up and down the typechecker stack.

The refactor keeps the same GLSL -> HIR -> LPIR -> native codegen pipeline. The output should be behaviorally identical, but stack frames and heap allocation patterns should be easier to reason about.

## File Structure

```text
lp-shader/
  lps-glsl/
    src/
      hir.rs
      hir/
        arena.rs              # new: ExprId, PlaceId, ExprList, HirArena
        types.rs              # update: arena-backed HIR data and stmt IDs
        typeck.rs             # update: type_expr returns ExprId
        builtin.rs            # split large builtin typing helpers
        builtin_out.rs        # update for ExprId/PlaceId writebacks
        coerce.rs             # update: coercion creates arena nodes
        const_fold.rs         # update: folds through arena/views
        place.rs              # update: PlaceId path and compact segments
        scalar.rs
        shape.rs
        ...
      lower.rs                # update: lower_expr takes ExprId and arena
      lower/
        place/
          path.rs             # update: PlaceId/ExprId indexed places
          read.rs
          write.rs
          layout.rs
        ops/
          place_project.rs
          place_read.rs
          place_write.rs
          ...
```

## Architecture Summary

Current shape:

```text
ParsedExpr
  type_expr recursively returns HirExpr
    HirExpr owns children via Box<HirExpr> / Vec<HirExpr>
    HirExpr may inline HirAssignTarget
      HirAssignTarget owns HirPlace
        HirPlace may own Box<HirExpr> for index segments
```

Target shape:

```text
ParsedExpr
  type_expr recursively returns ExprId
    ExprId points into HirArena.exprs
    child expressions are ExprId or ExprList
    assignable places are PlaceId

HirFunctionBody
  owns locals
  owns statements with ExprId/PlaceId handles
  owns one HirArena for the function

lower_function
  reads body.arena
  lowers ExprId and PlaceId directly
```

## Main Components

### `HirArena`

Add a new `hir/arena.rs`.

Expected starting shape:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ExprId(u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PlaceId(u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExprList {
    start: u32,
    len: u16,
}

pub struct HirArena {
    exprs: Vec<HirExpr>,
    places: Vec<HirPlace>,
    expr_lists: Vec<ExprId>,
}
```

`ExprList` should be used for argument/child lists instead of storing a fresh `Vec<ExprId>` in every call-like expression. If implementation pressure makes that too much for the first expression phase, a `Vec<ExprId>` compatibility step is acceptable only temporarily and should be removed before final validation.

### `HirExpr`

Keep the public name if that minimizes churn, but change what it represents.

Current:

```rust
pub struct HirExpr {
    pub span: Span,
    pub ty: LpsType,
    pub kind: HirExprKind,
}
```

Target: the same data lives inside the arena, while external references use `ExprId`.

Expression children become IDs:

```rust
pub enum HirExprKind {
    Constructor { args: ExprList },
    Cast { expr: ExprId },
    Swizzle { base: ExprId, lanes: SmallLaneList },
    Index { base: ExprId, index: ExprId },
    Builtin { kind: BuiltinKind, args: ExprList, writebacks: Vec<HirUserCallWriteback> },
    UserCall { function: usize, args: ExprList, writebacks: Vec<HirUserCallWriteback> },
    ImportCall { import: ImportKey, args: ExprList, out: Option<HirOutArg> },
    TexelFetch { sampler: HirTextureOperand, coord: ExprId, lod: ExprId },
    Texture { sampler: HirTextureOperand, coord: ExprId, import: ImportKey },
    Unary { op: UnaryOp, expr: ExprId },
    Binary { op: BinaryOp, lhs: ExprId, rhs: ExprId },
    Sequence { first: ExprId, second: ExprId },
    Conditional { condition: ExprId, accept: ExprId, reject: ExprId },
    PlaceRead { target: PlaceId },
    Assign { target: PlaceId, value: ExprId },
    IncDec { target: PlaceId, op: IncDecOp, prefix: bool },
    ...
}
```

### `HirFunctionBody`

Move arena ownership to the function body:

```rust
pub struct HirFunctionBody {
    pub locals: Vec<HirLocal>,
    pub statements: Vec<HirStmt>,
    pub arena: HirArena,
}
```

Statements should store `ExprId`:

```rust
pub enum HirStmt {
    Let { local: usize, init: ExprId },
    Assign { local: usize, value: ExprId },
    If { condition: ExprId, accept: Vec<HirStmt>, reject: Vec<HirStmt> },
    ...
    Expr(ExprId),
    Return { expr: Option<ExprId>, span: Span },
}
```

### `TypeCtx`

`TypeCtx` should own the function's `HirArena`.

Current central API:

```rust
fn type_expr(&mut self, expr: &ParsedExpr) -> Result<HirExpr, Diagnostic>
```

Target:

```rust
fn type_expr(&mut self, expr: &ParsedExpr) -> Result<ExprId, Diagnostic>
```

Type checks should inspect types through helpers:

```rust
fn expr(&self, id: ExprId) -> &HirExpr;
fn expr_ty(&self, id: ExprId) -> &LpsType;
fn expr_span(&self, id: ExprId) -> Span;
```

Coercions become arena-building operations:

```rust
fn coerce_expr(&mut self, expr: ExprId, target: &LpsType) -> Result<ExprId, Diagnostic>;
```

### Builtin Typing

Split `type_builtin_args` and `type_glsl_import_args` into smaller helpers before the arena migration.

The current monolithic `type_builtin_args` is the largest known stack frame. Splitting it gives an immediate safety improvement and makes the later arena-aware conversion easier to review.

Suggested helper families:

- arity and small argument destructuring helpers.
- unary numeric builtins.
- integer builtins.
- vector relational builtins.
- matrix builtins.
- mixed arity builtins such as `clamp`, `mix`, `smoothstep`, `fma`.
- GLSL imports split separately from regular builtins.

### Places

After expression IDs are in place, move assignable places into the same arena.

Target:

```rust
pub struct HirPlace {
    pub root: PlaceRoot,
    pub segments: Vec<PlaceSegment>,
    pub ty: LpsType,
}

pub enum PlaceSegment {
    Field {
        name: String,
        ty: LpsType,
        lane_offset: u8,
        lane_count: u8,
        byte_offset: u16,
    },
    Swizzle {
        fields: String,
        lanes: SmallLaneList,
        ty: LpsType,
    },
    Index {
        index: ExprId,
        ty: LpsType,
    },
}
```

Keep field names until texture path handling has an alternate representation. Texture operands currently use field names to build sampler paths.

### Lowering

Lowering should consume arena-backed HIR directly.

Current:

```rust
fn lower_expr(ctx: &mut LowerCtx<'_>, expr: &HirExpr) -> Result<LowerValue, Diagnostic>
```

Target:

```rust
fn lower_expr(
    ctx: &mut LowerCtx<'_>,
    arena: &HirArena,
    expr: ExprId,
) -> Result<LowerValue, Diagnostic>
```

Place lowering similarly accepts `PlaceId` or a resolved `&HirPlace` from the arena.

Do not add a permanent freeze step from arena HIR back to recursive HIR. A short-lived migration helper is acceptable only inside a phase if the phase removes it before completion.

## Behavioral Decisions

- Preserve all existing tests and shader behavior.
- Prefer smaller IDs and range handles on recursive paths.
- Keep recursive typechecking in the first pass, but make each recursive frame small.
- Use direct loops instead of iterator `collect()` in hot helpers when it avoids hidden stack or allocation costs.
- Keep diagnostics at the same quality level where possible.
- Avoid over-optimizing strings and maps until expression/place size is fixed.

## Validation Strategy

Each phase should validate host behavior first, then RV32 build behavior, then device behavior when the phase changes firmware-relevant compiler code.

Plan-level final validation:

```bash
cargo fmt --check
cargo test -p fw-tests --test scene_render_emu --test profile_alloc_emu
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu
cargo check -p lpa-server
cargo test -p lpa-server --no-run
just demo-esp32c6-check basic
```

If a device is not attached, record that `just demo-esp32c6-check basic` was not run and do not call the plan complete.
