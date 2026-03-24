# Phase 3: Rename LPIR Import Modules and Update Consumers

## Scope

Rename `"std.math"` LPIR import module to `"glsl"` and `"lpir"` (based on
classification), update all consumers: WASM emitter import resolution,
interpreter handler, Naga lowering, and test files.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### 3a. Split `register_std_math_imports` in `lower.rs`

Current function registers all math imports under `"std.math"`. Split into
two module names based on classification:

```rust
fn register_math_imports(mb: &mut ModuleBuilder) -> BTreeMap<String, CalleeRef> {
    let mut m = BTreeMap::new();
    let f1 = &[IrType::F32];
    let r1 = &[IrType::F32];

    // Helper that registers with the given module name
    let mut reg = |module: &str, name: &str, params: &[IrType], rets: &[IrType]| {
        let r = mb.add_import(ImportDecl {
            module_name: String::from(module),
            func_name: String::from(name),
            param_types: params.to_vec(),
            return_types: rets.to_vec(),
            lpfx_glsl_params: None,
        });
        m.insert(format!("{module}::{name}"), r);
    };

    // lpir module — IR ops needing library impl
    reg("lpir", "sqrt", f1, r1);
    // roundeven not currently registered, add when needed

    // glsl module — GLSL std functions
    reg("glsl", "sin", f1, r1);
    reg("glsl", "cos", f1, r1);
    reg("glsl", "tan", f1, r1);
    reg("glsl", "asin", f1, r1);
    reg("glsl", "acos", f1, r1);
    reg("glsl", "atan", f1, r1);
    reg("glsl", "atan2", &[IrType::F32, IrType::F32], r1);
    reg("glsl", "sinh", f1, r1);
    reg("glsl", "cosh", f1, r1);
    reg("glsl", "tanh", f1, r1);
    reg("glsl", "asinh", f1, r1);
    reg("glsl", "acosh", f1, r1);
    reg("glsl", "atanh", f1, r1);
    reg("glsl", "exp", f1, r1);
    reg("glsl", "exp2", f1, r1);
    reg("glsl", "log", f1, r1);
    reg("glsl", "log2", f1, r1);
    reg("glsl", "pow", &[IrType::F32, IrType::F32], r1);
    reg("glsl", "ldexp", &[IrType::F32, IrType::I32], r1);
    reg("glsl", "round", f1, r1);

    m
}
```

Rename the function from `register_std_math_imports` to `register_math_imports`
(or similar). Update the call site in `lower()`.

### 3b. Update `lower_math.rs`

`push_std_math` uses key format `"std.math::{name}"`. Update to accept module:

```rust
fn push_import_call(
    ctx: &mut LowerCtx<'_>,
    module: &str,
    name: &str,
    args: &[VReg],
) -> Result<VReg, LowerError> {
    let key = format!("{module}::{name}");
    let callee = ctx
        .import_map
        .get(&key)
        .copied()
        .ok_or_else(|| LowerError::Internal(format!("missing import {key}")))?;
    let r = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push_call(callee, args, &[r]);
    Ok(r)
}
```

Then update all call sites. Most calls currently use `push_std_math(ctx, "sin", &[v])`
etc. Change to `push_import_call(ctx, "glsl", "sin", &[v])`. For sqrt:
`push_import_call(ctx, "lpir", "sqrt", &[v])`.

Search for all uses of the old `push_std_math` to find every call site.

### 3c. Update `lower_ctx.rs`

The import map keys change from `"std.math::{name}"` to `"glsl::{name}"` or
`"lpir::{name}"`. `LowerCtx` stores this as `import_map: BTreeMap<String, CalleeRef>`,
so the keys are just strings — the struct itself doesn't need changes,
only the callers that construct keys.

### 3d. Update WASM emitter `imports.rs`

`resolve_builtin_id` currently matches on module names:

```rust
fn resolve_builtin_id(decl: &ImportDecl) -> Result<BuiltinId, String> {
    match decl.module_name.as_str() {
        "std.math" => { ... }
        "lpfx" => { ... }
        m => Err(...)
    }
}
```

Update to match on `"glsl"`, `"lpir"`, and `"lpfx"`:

```rust
fn resolve_builtin_id(decl: &ImportDecl) -> Result<BuiltinId, String> {
    match decl.module_name.as_str() {
        "glsl" => {
            let ac = decl.param_types.len();
            glsl_q32_math_builtin_id(decl.func_name.as_str(), ac)
                .ok_or_else(|| format!("unsupported glsl import `{}`", decl.func_name))
        }
        "lpir" => {
            let ac = decl.param_types.len();
            lpir_q32_builtin_id(decl.func_name.as_str(), ac)
                .ok_or_else(|| format!("unsupported lpir import `{}`", decl.func_name))
        }
        "lpfx" => {
            // ... same as before, just updated variant names ...
        }
        m => Err(format!("unsupported import module `{m}`")),
    }
}
```

The `lpir_q32_builtin_id` function needs to be available — either as a new
generated function in `glsl_builtin_mapping.rs` or manually written. Currently
the only `lpir` import is `sqrt`. A simple manual match is fine for now:

```rust
// In imports.rs or as a new function in glsl_builtin_mapping.rs
fn lpir_q32_builtin_id(name: &str, _arg_count: usize) -> Option<BuiltinId> {
    match name {
        "sqrt" => Some(BuiltinId::LpLpirFsqrtQ32),
        _ => None,
    }
}
```

Also update `std_math_callee` (used by WASM emitter for Op::Fsqrt/Fnearest
routing through imports). It currently searches for `d.module_name == "std.math"`.
Update to search the appropriate module:

```rust
pub(crate) fn import_callee(ir: &IrModule, module: &str, func_name: &str)
    -> Result<CalleeRef, String>
{
    ir.imports
        .iter()
        .enumerate()
        .find(|(_, d)| d.module_name == module && d.func_name == func_name)
        .map(|(i, _)| CalleeRef(i as u32))
        .ok_or_else(|| format!("missing import @{module}::{func_name}"))
}
```

Update call sites in `ops.rs`:
- `std_math_callee(ir, "sqrt")` → `import_callee(ir, "lpir", "sqrt")`
- `std_math_callee(ir, "round")` → `import_callee(ir, "glsl", "round")`

### 3e. Rename `StdMathHandler`

In `lp-glsl-naga/src/std_math_handler.rs`:

Rename `StdMathHandler` → `BuiltinImportHandler` (or `GlslLpirHandler`).
Update to dispatch on both `"glsl"` and `"lpir"` module names:

```rust
pub struct BuiltinImportHandler;

impl ImportHandler for BuiltinImportHandler {
    fn call(
        &mut self,
        module_name: &str,
        func_name: &str,
        args: &[Value],
    ) -> Result<Vec<Value>, InterpError> {
        match module_name {
            "glsl" => self.dispatch_glsl(func_name, args),
            "lpir" => self.dispatch_lpir(func_name, args),
            _ => Err(InterpError::Import(format!("unknown module {module_name}"))),
        }
    }
}
```

`dispatch_glsl` contains the existing math dispatch (sin, cos, etc.).
`dispatch_lpir` handles `sqrt` (and future lpir imports):

```rust
fn dispatch_lpir(&self, func_name: &str, args: &[Value]) -> ... {
    match func_name {
        "sqrt" => Ok(vec![Value::F32(libm::sqrtf(f(args, 0)?))]),
        _ => Err(...)
    }
}
```

Update re-export in `lib.rs` if the type name changes. Update all uses
(tests, `CombinedImports` in `lower_interp.rs`).

### 3f. Update LPIR test files

**`lpir/src/tests.rs`** — hand-written LPIR text strings:
- `@std.math::fsin` → `@glsl::fsin` (note: these use `fsin` not `sin`)
- Check whether `fsin` should become `sin` to match the new convention.
  The Naga lowering uses `sin` as the func_name. If the hand-written tests
  use different names (`fsin`, `fabs`, `fmax`), update them to match the
  import names used by the lowering. The `MockMathImports` handler needs
  to match the same names.

**`lpir/src/tests/interp.rs`** — hand-written LPIR + mock handlers:
- `@std.math::fabs` → `@glsl::fabs` (or `@glsl::abs` if normalizing)
- `@std.math::fmax` → `@glsl::fmax` (or `@glsl::max`)
- `@std.math::unknown` → `@glsl::unknown`
- `("std.math", "fabs")` → `("glsl", "fabs")`
- `("std.math", "fmax")` → `("glsl", "fmax")`
- `("std.math", "unknown")` → `("glsl", "unknown")`

These are standalone test imports — the names don't need to match the real
builtins exactly, they just need to be internally consistent within each test.
Replace `std.math` with `glsl` in all of them.

**`lp-glsl-naga/tests/lower_print.rs`**:
- `assert!(s.contains("import @std.math::"), "{s}");` → update to check for
  `"import @glsl::"` or `"import @lpir::"`
- `assert!(s.contains("call @std.math::"), "{s}");` → similar

**`lp-glsl-naga/tests/lower_interp.rs`**:
- `CombinedImports` delegates to `StdMathHandler` — update to use renamed
  handler and match on `"glsl"` / `"lpir"` module names.

### 3g. Update generator comment

In `lp-glsl-builtins-gen-app/src/main.rs` line ~301, there's a comment:
```
// GLSL: `atan(y, x)`; Naga lowers two-arg atan as `std.math::atan2`.
```
Update to `glsl::atan2`.

## Validate

```
cargo check -p lp-glsl-naga
cargo test -p lp-glsl-naga
cargo check -p lp-glsl-wasm
cargo test -p lp-glsl-wasm
cargo check -p lpir
cargo test -p lpir
```

Then run the full WASM-path filetests:
```
just test-glsl-filetests
```

The old cranelift path tests will fail — that's expected and accepted.
