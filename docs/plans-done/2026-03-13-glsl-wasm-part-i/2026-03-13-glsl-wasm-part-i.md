# Part i: Extract lp-glsl-frontend

Roadmap: `docs/roadmaps/2026-03-13-glsl-wasm-playground/`

## Scope

Extract the shared, target-independent frontend code from lp-glsl-compiler
into a new crate `lp-glsl-frontend`. Rename `lp-glsl-compiler` to
`lp-glsl-cranelift`. All existing tests pass unchanged. No new functionality.

This is a mechanical refactor that establishes the crate boundary needed
for the WASM backend (part ii) to depend on shared types without pulling
in Cranelift.

## Cranelift coupling points in the frontend

Three files in the would-be-frontend currently reference Cranelift types.
These must be decoupled before extraction.

### 1. `frontend/semantic/types.rs` — `Type::to_cranelift_type()`

The `Type` enum has a `to_cranelift_type()` method that maps GLSL types to
`cranelift_codegen::ir::Type`. This is the only Cranelift dependency in
the core type system.

**Fix**: Remove `to_cranelift_type()` from `Type`. Add a free function or
extension trait in `lp-glsl-cranelift` that provides the same mapping.
Callers are all in the codegen (context.rs, helpers.rs) — update them to
use the new location. The WASM backend will have its own equivalent
mapping (GLSL Type → WASM value type).

### 2. `frontend/semantic/lpfx/lpfx_sig.rs` — Cranelift signature building

`lpfx_sig.rs` contains functions that build `cranelift_codegen::ir::Signature`
objects and work with `cranelift_codegen::ir::Value`. This is codegen
support code that was placed in the semantic directory.

**Fix**: Move `lpfx_sig.rs` to `lp-glsl-cranelift` (into the codegen
directory alongside `lpfx_fns.rs`). It's only used by the Cranelift
codegen path.

### 3. `frontend/src_loc_manager.rs` — `cranelift_codegen::ir::SourceLoc`

`SourceLocManager` maps `cranelift_codegen::ir::SourceLoc` (an opaque u32)
to GLSL source positions. This is used for error reporting on trap
locations (division by zero, etc.).

**Fix**: Replace the Cranelift `SourceLoc` type with a simple newtype
wrapper `u32` in lp-glsl-frontend. The manager just needs an opaque ID →
position mapping. lp-glsl-cranelift can convert between the frontend's
ID and Cranelift's `SourceLoc` at the boundary (they're both u32). The
WASM backend can reuse the same source location manager for its own
error reporting.

## What moves to lp-glsl-frontend

```
lp-glsl-frontend/src/
├── lib.rs
├── error.rs                  # from error/ (GlslError, GlslDiagnostics, ErrorCode)
├── pipeline.rs               # from frontend/pipeline.rs
├── src_loc.rs                # from frontend/src_loc.rs (GlSourceMap, GlSourceLoc)
├── src_loc_manager.rs        # from frontend/src_loc_manager.rs (decoupled from Cranelift)
└── semantic/                 # from frontend/semantic/ (all files except lpfx_sig.rs)
    ├── mod.rs                #   TypedShader, TypedFunction, SemanticAnalyzer
    ├── types.rs              #   Type enum (without to_cranelift_type)
    ├── functions.rs          #   FunctionRegistry, FunctionSignature, Parameter
    ├── type_check/           #   mod.rs, operators.rs, conversion.rs, swizzle.rs,
    │                         #   constructors.rs, matrix.rs, inference.rs
    ├── const_eval.rs
    ├── type_resolver.rs
    ├── scope.rs
    ├── builtins.rs
    ├── validator.rs
    ├── lpfx/                 #   mod.rs, lpfx_fn.rs, lpfx_fn_registry.rs, lpfx_fns.rs
    │                         #   (NOT lpfx_sig.rs — that moves to cranelift)
    └── passes/               #   mod.rs, global_const_pass.rs, function_registry.rs,
                              #   function_extraction.rs, function_signature.rs,
                              #   validation.rs
```

## What stays in lp-glsl-cranelift (renamed from lp-glsl-compiler)

Everything currently in `frontend/codegen/`, `frontend/glsl_compiler.rs`,
`backend/`, and `exec/`. Plus `lpfx_sig.rs` moved from semantic to codegen.

The crate adds `lp-glsl-frontend` as a dependency and re-imports from it
where needed.

## Dependencies

**lp-glsl-frontend**:
- `glsl` (parser)
- `hashbrown`
- `serde` (no_std, alloc)
- `log`
- `lp-model` (for GlslOpts, Q32Options)
- NO cranelift crates

**lp-glsl-cranelift** (renamed):
- `lp-glsl-frontend` (new dependency)
- `cranelift-codegen`, `cranelift-frontend`, `cranelift-jit`,
  `cranelift-module`, `cranelift-object` (existing)
- `lp-glsl-builtins`, `lp-glsl-jit-util` (existing)
- everything else it has today

## Workspace changes

### Cargo.toml (workspace root)

Members: replace `lp-glsl/lp-glsl-compiler` with:
```
"lp-glsl/lp-glsl-frontend",
"lp-glsl/lp-glsl-cranelift",
```

Default-members: same replacement.

### Downstream crates that depend on lp-glsl-compiler

These need their dependency renamed to `lp-glsl-cranelift`:
- `lp-core/lp-engine` (uses glsl_jit, glsl_jit_streaming, GlslExecutable)
- `lp-glsl/lp-glsl-filetests` (uses compilation APIs)
- `lp-glsl/lp-glsl-filetests-app` (binary)
- `lp-glsl/lp-glsl-filetests-gen-app` (binary)
- `lp-glsl/lp-glsl-q32-metrics-app` (binary)
- Any other crate that imports from `lp_glsl_compiler`

Some of these may also want to depend on `lp-glsl-frontend` directly for
types like `TypedShader`, `GlslError`, etc. Others only need the cranelift
crate's public API.

## Phases (within this plan)

### Phase 1: Decouple Cranelift from frontend code

1. Remove `to_cranelift_type()` from `Type` in `types.rs`. Add equivalent
   function in the codegen (e.g. `codegen/types.rs` or as methods on
   `CodegenContext`). Update all callers in the codegen.

2. Move `lpfx_sig.rs` from `frontend/semantic/lpfx/` to
   `frontend/codegen/`. Update imports in `codegen/lpfx_fns.rs` and
   anywhere else that uses it.

3. Replace `cranelift_codegen::ir::SourceLoc` in `src_loc_manager.rs`
   with a local `SourceLocId(u32)` newtype. Add a
   `SourceLocId::to_cranelift_srcloc()` conversion method behind a
   feature or in lp-glsl-cranelift.

4. Verify: `cargo build` and `cargo test` pass. No behavioral changes.

### Phase 2: Create lp-glsl-frontend crate

1. Create `lp-glsl/lp-glsl-frontend/Cargo.toml` with the dependencies
   listed above.

2. Move the files listed in "What moves" above. This is `git mv` for
   each file/directory.

3. Set up `lib.rs` with the module structure and public re-exports.

4. Verify the new crate compiles: `cargo build -p lp-glsl-frontend`.

### Phase 3: Rename lp-glsl-compiler to lp-glsl-cranelift

1. Rename the directory: `lp-glsl/lp-glsl-compiler/` →
   `lp-glsl/lp-glsl-cranelift/`.

2. Update `Cargo.toml` (package name, lib name).

3. Add `lp-glsl-frontend` as a dependency.

4. Update all `use crate::frontend::semantic::*` to
   `use lp_glsl_frontend::semantic::*` (and similar for error, pipeline,
   src_loc).

5. Update all `use crate::error::*` to `use lp_glsl_frontend::error::*`.

6. Verify: `cargo build -p lp-glsl-cranelift`.

### Phase 4: Update downstream crates

1. Update workspace `Cargo.toml` (members, default-members).

2. Update every crate that depended on `lp-glsl-compiler`:
   - Change dependency name to `lp-glsl-cranelift`
   - Add `lp-glsl-frontend` dependency where needed for shared types
   - Update `use` statements

3. Verify: `cargo build` (full workspace) and `cargo test` pass.

### Phase 5: Cleanup

1. Verify all tests pass: `cargo test`.
2. Run `cargo +nightly fmt`.
3. Fix any warnings.
4. Verify `just build-fw-esp32` still works (it depends on lp-engine
   which depends on the compiler).
5. Verify `just build-builtins` still works.

## Validate

```
cargo build
cargo test
cargo +nightly fmt --check
just build-fw-esp32
just build-builtins
```

All must pass. The refactor is purely structural — zero behavioral changes.

## Risk

Low. Every phase is a mechanical transformation. The Cranelift decoupling
(phase 1) is the only part that requires judgment; the rest is
move-and-rename.

The `to_cranelift_type` removal is the highest-risk change. It touches
callers in ~3-4 codegen files. If a caller is missed, the compiler will
catch it (it's a type error, not a runtime error).
