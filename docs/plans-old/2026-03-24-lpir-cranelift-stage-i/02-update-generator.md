# Phase 2: Update Generator and Regenerate

## Scope

Update `lps-builtins-gen-app` to understand the new naming convention,
generate `Module` and `Mode` enums, add self-describing methods to `BuiltinId`,
remove old cranelift outputs, and regenerate all files.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### 2a. Remove old cranelift outputs from generator

In `main.rs`, remove:
- `generate_registry` call and function (~lines 51–58, 730+)
- `generate_testcase_mapping` call and function (~lines 88–95)
- The `registry_path` and `mapping_rs_path` variables
- Remove these paths from the `format_generated_files` call

This is ~300 lines of deletion. The `lps-cranelift` crate will no longer
compile after this (accepted).

### 2b. Add module/mode parsing to BuiltinInfo

Add fields to `BuiltinInfo`:

```rust
struct BuiltinInfo {
    // ... existing fields ...
    /// Builtin module: "lpir", "glsl", or "lpfx"
    builtin_module: String,
    /// Function name within the module (e.g., "sin", "fadd", "fbm2")
    builtin_fn_name: String,
    /// Float mode suffix: Some("q32"), Some("f32"), or None
    builtin_mode: Option<String>,
}
```

In `extract_builtin`, after confirming the function starts with `__`, parse
the new naming convention:

```rust
// All builtins now start with __lp_
// Format: __lp_<module>_<fn>_<mode>  or  __lp_<module>_<fn> (no mode)
let after_lp = func_name.strip_prefix("__lp_")?;

// Module is the first segment: "lpir", "glsl", or "lpfx"
let (module, rest) = if after_lp.starts_with("lpir_") {
    ("lpir", &after_lp[5..])
} else if after_lp.starts_with("glsl_") {
    ("glsl", &after_lp[5..])
} else if after_lp.starts_with("lpfx_") {
    ("lpfx", &after_lp[5..])
} else {
    return None; // Unknown module
};

// Mode is _q32 or _f32 suffix (if present)
let (fn_name, mode) = if rest.ends_with("_q32") {
    (&rest[..rest.len()-4], Some("q32"))
} else if rest.ends_with("_f32") {
    (&rest[..rest.len()-4], Some("f32"))
} else {
    (rest, None)
};
```

### 2c. Generate Module and Mode enums

Add to the generated `lib.rs`:

```rust
/// Builtin module classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Module {
    /// IR ops needing library impl (fadd, fsub, fmul, fdiv, fsqrt, fnearest)
    Lpir,
    /// GLSL std.450 functions (sin, cos, pow, exp, ...)
    Glsl,
    /// LightPlayer effects (fbm, snoise, hash, ...)
    Lpfx,
}

/// Float mode for mode-dependent builtins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mode {
    Q32,
    F32,
}
```

### 2d. Generate self-describing methods

Add to the generated `impl BuiltinId`:

**`module()`** — generated match returning `Module::Lpir/Glsl/Lpfx`:

```rust
pub fn module(&self) -> Module {
    match self {
        BuiltinId::LpLpirFaddQ32 => Module::Lpir,
        BuiltinId::LpGlslSinQ32 => Module::Glsl,
        BuiltinId::LpLpfxFbm2Q32 => Module::Lpfx,
        // ...
    }
}
```

**`fn_name()`** — generated match returning the function name within the
module:

```rust
pub fn fn_name(&self) -> &'static str {
    match self {
        BuiltinId::LpLpirFaddQ32 => "fadd",
        BuiltinId::LpGlslSinQ32 => "sin",
        BuiltinId::LpLpfxFbm2Q32 => "fbm2",
        BuiltinId::LpLpfxHash1 => "hash_1",
        // ...
    }
}
```

**`mode()`** — generated match returning `Option<Mode>`:

```rust
pub fn mode(&self) -> Option<Mode> {
    match self {
        BuiltinId::LpGlslSinQ32 => Some(Mode::Q32),
        BuiltinId::LpLpfxHash1 => None,  // mode-independent
        // ...
    }
}
```

### 2e. Update re-exports

The generated `lib.rs` currently re-exports `GlslParamKind`,
`glsl_q32_math_builtin_id`, `glsl_lpfx_q32_builtin_id`. Add re-exports for
`Module` and `Mode` (these are defined inline in `lib.rs`, not in a
submodule, so no re-export needed — just make sure they're `pub`).

### 2f. Update glsl_builtin_mapping generation

The `generate_glsl_builtin_mapping` function generates
`glsl_q32_math_builtin_id` and `glsl_lpfx_q32_builtin_id`. These use enum
variant names, which change with the rename:
- `BuiltinId::LpQ32Sin` → `BuiltinId::LpGlslSinQ32`
- `BuiltinId::LpfxFbm2Q32` → `BuiltinId::LpLpfxFbm2Q32`

The generator already reads variant names from `BuiltinInfo`, so this should
happen automatically once Phase 1 renames are in place and the generator
re-derives variant names.

The match key names in `glsl_q32_math_builtin_id` (e.g. `"sin"`, `"add"`)
need review. Currently `"add"` maps to `LpQ32Add`. After rename, the generator
should use the `builtin_fn_name` from parsing — but the match keys are the
GLSL-facing names used by import resolution, not the builtin fn names. For
`lpir` builtins like `fadd`, the GLSL/import-facing name might differ from
the fn_name. Check whether `glsl_q32_math_builtin_id` is still needed for
`lpir` builtins — it may only be needed for `glsl` module builtins. The
`lpir` builtins can be resolved via a separate function or the same function
with `lpir` fn names (`"sqrt"` → `LpLpirFsqrtQ32`).

Alternatively: since we're splitting imports into `"glsl"` and `"lpir"`
modules in Phase 3, the WASM import resolution will match on module name
first. The `glsl_q32_math_builtin_id` function could be split into
`glsl_q32_builtin_id(name, arity)` and `lpir_q32_builtin_id(name, arity)`.
Or keep one function that handles both. The generator should emit the
appropriate mapping.

### 2g. Run generator

```
cargo run -p lps-builtins-gen-app
```

This regenerates:
- `lps-builtin-ids/src/lib.rs`
- `lps-builtin-ids/src/glsl_builtin_mapping.rs`
- `lps-builtins-emu-app/src/builtin_refs.rs`
- `lps-builtins-wasm/src/builtin_refs.rs`
- `lps-builtins/src/builtins/q32/mod.rs`
- `lps-wasm/src/codegen/builtin_wasm_import_types.rs`

(No longer generates `registry.rs` or `mapping.rs` for old cranelift.)

### 2h. Verify generated output

Spot-check the generated `lib.rs`:
- Variant names look right (`LpGlslSinQ32`, `LpLpirFaddQ32`, etc.)
- `name()` returns correct symbol strings
- `module()`, `fn_name()`, `mode()` return correct values
- `builtin_id_from_name()` round-trips correctly

## Validate

```
cargo check -p lps-builtin-ids
cargo test -p lps-builtin-ids
cargo check -p lps-builtins
cargo test -p lps-builtins
cargo check -p lps-builtins-gen-app
cargo test -p lps-builtins-gen-app
```

The WASM emitter and Naga crates may still fail (they reference `"std.math"`
and old `BuiltinId` variant names in non-generated code). That's Phase 3.
