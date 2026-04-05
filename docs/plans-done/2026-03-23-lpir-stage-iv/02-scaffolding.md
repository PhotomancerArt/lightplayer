# Phase 2: Scaffolding

## Scope

Set up the crate dependencies, module structure, error type, and the
`lower()` entry point. By the end of this phase, `lower()` compiles and
returns an empty `IrModule` (no functions lowered yet). The `LowerCtx`
struct is defined but not fully populated.

## Code Organization Reminders

- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment.

## Implementation Details

### `Cargo.toml` — add dependencies

```toml
[dependencies]
naga = { version = "29.0.0", default-features = false, features = ["glsl-in"] }
lpir = { path = "../lpir" }
lps-builtin-ids = { path = "../lps-builtin-ids" }
```

### `lib.rs` — expose new modules

Add public module declarations:

```rust
pub mod lower;
mod lower_ctx;
mod lower_expr;
mod lower_stmt;
mod lower_math;
mod lower_lpfx;
pub mod std_math_handler;
```

`lower` and `std_math_handler` are `pub` (consumers need the entry point
and tests need the handler). The rest are private implementation modules.

### `lower.rs` — entry point

```rust
pub fn lower(naga_module: &NagaModule) -> Result<lpir::IrModule, LowerError>
```

`LowerError`:
```rust
#[derive(Debug)]
pub enum LowerError {
    UnsupportedExpression(String),
    UnsupportedStatement(String),
    UnsupportedType(String),
    Internal(String),
}
```

Implement `Display` and `Error` for `LowerError`.

Initial implementation:
1. Create `ModuleBuilder::new()`
2. Build function index map: `Handle<Function>` → anticipated `CalleeRef`
   (imports come first, then functions in order)
3. For each `(handle, function_info)` in `naga_module.functions`:
   - Create `FunctionBuilder::new(name, return_types)`
   - Set entry if applicable
   - Call `finish()` and `mb.add_function()`
4. Return `mb.finish()`

The function body lowering is stubbed (empty body, just return).

### `lower_ctx.rs` — per-function context

```rust
pub(crate) struct LowerCtx<'a> {
    pub fb: FunctionBuilder,
    pub module: &'a naga::Module,
    pub func: &'a naga::Function,
    pub ir_module: &'a IrModule,  // for callee resolution (not yet available — use module builder)
    pub expr_cache: Vec<Option<VReg>>,
    pub local_map: BTreeMap<Handle<LocalVariable>, VReg>,
    pub param_aliases: BTreeMap<Handle<LocalVariable>, VReg>,
    pub func_map: BTreeMap<Handle<Function>, CalleeRef>,
    pub import_map: BTreeMap<String, CalleeRef>,  // "module::name" → CalleeRef
}
```

Implement `LowerCtx::new()`:
1. Create `FunctionBuilder`
2. Add params (mapping Naga `FunctionArgument` types to LPIR types)
3. Detect parameter aliases (scan body for `Store(LocalVar, FuncArg)`)
4. Allocate VRegs for non-aliased local variables
5. Initialize `expr_cache` with `None` for each expression in the arena

Helper methods:
- `naga_scalar_to_ir_type(ScalarKind) -> Result<IrType, LowerError>`:
  Float→F32, Sint/Uint/Bool→I32
- `naga_type_to_ir_type(&TypeInner) -> Result<IrType, LowerError>`:
  scalar only (vectors error for now)
- `resolve_local(Handle<LocalVariable>) -> VReg`: checks alias map first
- `ensure_expr(Handle<Expression>) -> Result<VReg, LowerError>`:
  checks cache, calls lower_expr if miss (stubbed)

### Stub files

Create empty stub files so the crate compiles:

- `lower_expr.rs`: `pub(crate) fn lower_expr(ctx: &mut LowerCtx, expr: Handle<Expression>) -> Result<VReg, LowerError> { todo!() }`
- `lower_stmt.rs`: `pub(crate) fn lower_block(ctx: &mut LowerCtx, block: &Block) -> Result<(), LowerError> { todo!() }`
- `lower_math.rs`: empty
- `lower_lpfx.rs`: empty
- `std_math_handler.rs`: empty struct implementing `ImportHandler` with `todo!()`

## Validate

```
cargo check -p lps-naga
cargo test -p lps-naga
cargo +nightly fmt -p lps-naga -- --check
```

Existing `lps-naga` tests must still pass. The `lower()` function
compiles but produces empty function bodies.
