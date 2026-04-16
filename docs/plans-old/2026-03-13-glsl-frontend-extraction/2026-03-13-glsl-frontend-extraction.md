# Extract lps-frontend (finish Part i)

Roadmap: `docs/roadmaps/2026-03-13-glsl-wasm-playground/`
Builds on: Phase 1 of `docs/plans/2026-03-13-glsl-wasm-part-i/` (complete)

## Design

### Scope

Complete the lps-frontend extraction: create lps-builtin-ids, create lps-frontend,
rename lps-compiler to lps-cranelift, move shared code, update downstream. Phase 1 (
Cranelift decoupling) is done.

### File structure

```
lp-shader/
├── lps-builtin-ids/           # NEW: shared BuiltinId enum
│   ├── Cargo.toml
│   └── src/
│       └── lib.rs                 # enum + name() + builtin_id_from_name() + all() (generated)
├── lps-frontend/              # NEW: parser, semantic, types, errors (no Cranelift)
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── error.rs
│       ├── pipeline.rs
│       ├── src_loc.rs
│       ├── src_loc_manager.rs
│       └── semantic/              # types, functions, type_check, lpfx, passes, etc.
├── lps-cranelift/             # RENAMED from lps-compiler
│   └── ...                        # codegen, backend, exec; depends on frontend + builtin-ids
├── lps-compiler/              # DELETED (renamed to cranelift)
└── lps-builtins-gen-app/      # UPDATE: emit to builtin-ids + cranelift registry
```

### Conceptual architecture

```
lps-builtin-ids (no deps on cranelift/lp-core)
    ↑                    ↑
    |                    |
lps-frontend    lps-cranelift
    ↑                    |
    |                    |
    +--------------------+
         (frontend depends on builtin-ids for LpfxFn; cranelift depends on both)
```

- **lps-builtin-ids**: Enum `BuiltinId`, `name()`, `builtin_id_from_name()`, `all()`. No
  format(), no Cranelift, no lp-model.
- **lps-frontend**: Error, pipeline, src_loc, semantic (including lpfx with BuiltinId from
  builtin-ids). DEFAULT_MAX_ERRORS.
- **lps-cranelift**: Re-exports BuiltinId from builtin-ids, adds format(), signature(),
  declare_*. Depends on frontend + builtin-ids.

## Phases

### Phase 1: Create lps-builtin-ids + update gen-app

**Scope**: New crate with enum and `name()` / `builtin_id_from_name()` / `all()`. Gen-app emits
there; cranelift registry re-exports and adds format/signature/declare.

**Code organization reminders**: One file in builtin-ids. Place enum first, then impl. Keep
builtin-ids minimal: no_std, no cranelift, no lp-model.

**Implementation details**:

1. Create `lp-shader/lps-builtin-ids/Cargo.toml`:
    - `edition.workspace`, `version.workspace`, etc.
    - No cranelift or lp-model deps.

2. Add `generate_builtin_ids()` in gen-app that emits to `lps-builtin-ids/src/lib.rs`:
    - Enum variants
    - `name()` method
    - `builtin_id_from_name()` method
    - `all()` returning `&'static [BuiltinId]`
    - No `format()`, no `signature()`, no `declare_*`

3. Refactor `generate_registry()`:
    - Emit `pub use lps_builtin_ids::BuiltinId;`
    - Add `format()` via trait or extension module: `impl BuiltinIdFormat for BuiltinId` in
      cranelift (trait in registry, impl uses `crate::DecimalFormat`)
    - Keep `signature()`, `declare_for_jit()`, `declare_for_emulator()` — they stay in registry
    - Update `all()` to delegate to `lps_builtin_ids::BuiltinId::all()` or remove if redundant

4. Add lps-builtin-ids to workspace members and default-members.

5. Update lps-compiler Cargo.toml: add `lps-builtin-ids` dependency.

6. Update compiler's backend/builtins to use `lps_builtin_ids::BuiltinId` (or re-export). Update
   lpfx_fns.rs gen to use `lps_builtin_ids::BuiltinId`.

**Validate**: `cargo run -p lps-builtins-gen-app`, `cargo build -p lps-compiler`,
`cargo test -p lps-compiler --features std`

---

### Phase 2: Create lps-frontend crate

**Scope**: New crate with error, pipeline, src_loc, src_loc_manager, semantic. Move files from
lps-compiler.

**Code organization reminders**: Match Design file structure. Place entry points and public API
first; helpers at bottom. Keep related functionality grouped.

**Implementation details**:

1. Create `lp-shader/lps-frontend/Cargo.toml`:
    - Dependencies: `glsl`, `hashbrown`, `serde` (no_std, alloc), `log`, `lps-builtin-ids`
    - No cranelift, no lp-model (frontend stands alone within lps)

2. Move files (git mv or copy then delete):
    - `error.rs` from compiler src
    - `frontend/pipeline.rs` → `pipeline.rs`
    - `frontend/src_loc.rs` → `src_loc.rs`
    - `frontend/src_loc_manager.rs` → `src_loc_manager.rs`
    - `frontend/semantic/` → `semantic/` (entire dir, including lpfx)

3. Fix imports in moved files:
    - `crate::` → `lps_frontend::` or `crate::` (internal)
    - `crate::frontend::` → `crate::`
    - `crate::error` → `crate::error`
    - `crate::backend::builtins::BuiltinId` → `lps_builtin_ids::BuiltinId`

4. Add `DEFAULT_MAX_ERRORS` to frontend (from `exec/executable.rs`). Update
   `GlslDiagnostics::From<GlslError>` and `semantic::analyze()` to use it.

5. Set up `lib.rs` with module structure and public re-exports.

6. Add lps-frontend to workspace.

7. Update gen-app: change `lpfx_fns_path` from `lps-compiler/.../semantic/lpfx/lpfx_fns.rs` to
   `lps-frontend/.../semantic/lpfx/lpfx_fns.rs`. Ensure generated code uses
   `lps_builtin_ids::BuiltinId`. Re-run gen-app.

**Validate**: `cargo run -p lps-builtins-gen-app`, `cargo build -p lps-frontend`

---

### Phase 3: Rename lps-compiler to lps-cranelift

**Scope**: Rename crate, add lps-frontend dep, remove moved files from compiler, update all
imports.

**Code organization reminders**: Keep backward-compatible re-exports in lib.rs for API stability.

**Implementation details**:

1. Rename directory: `lp-shader/lps-compiler` → `lp-shader/lps-cranelift`

2. Update `Cargo.toml`: package name `lps-cranelift`, add `lps-frontend` dependency.

3. Delete moved files from cranelift (error, pipeline, src_loc, src_loc_manager, frontend/semantic).
   Keep codegen, backend, exec, glsl_compiler.

4. Update all `use crate::frontend::` to `use lps_frontend::`
5. Update `use crate::error` to `use lps_frontend::`
6. Update `use crate::frontend::semantic::` to `use lps_frontend::semantic::`
7. Re-export from lib.rs: `pub use lps_frontend::{DEFAULT_MAX_ERRORS, ...}` for backward
   compatibility.

8. Update backend/builtins/registry: ensure it re-exports BuiltinId and provides
   format/signature/declare.

**Validate**: `cargo build -p lps-cranelift`, `cargo test -p lps-cranelift --features std`

---

### Phase 4: Update workspace and downstream crates

**Scope**: Workspace Cargo.toml, lp-engine, lps-filetests, lps-filetests-app,
lps-q32-metrics-app, tests.

**Code organization reminders**: Update imports systematically; grep for `lps_compiler` to find
all usages.

**Implementation details**:

1. Workspace `Cargo.toml`: Replace `lps-compiler` with `lps-frontend` and
   `lps-cranelift` in members and default-members. Add `lps-builtin-ids`.

2. Downstream crates that depended on lps-compiler:
    - Change to `lps-cranelift`
    - Add `lps-frontend` where they need shared types (TypedShader, GlslError, etc.)
    - Update `use lps_compiler::` to `use lps_cranelift::` (or `lps_frontend::` as
      appropriate)

3. Crates to update: lp-core/lp-engine, lp-shader/lps-filetests,
   lp-shader/lps-filetests-app, lp-shader/lps-q32-metrics-app, esp32-glsl-jit (if it uses
   compiler), tests in lps-cranelift.

**Validate**: `cargo build`, `cargo test`, `just build-fw-esp32`

---

### Phase 5: Cleanup and validation

**Scope**: Remove TODOs, fix warnings, format, final validation.

**Code organization reminders**: Grep for temporary code before committing.

**Implementation details**:

1. Grep for TODO, debug println!, temporary code. Remove.

2. `cargo +nightly fmt`

3. Fix any remaining warnings in lps-frontend, lps-builtin-ids, lps-cranelift.

4. Run: `cargo build`, `cargo test`, `cargo +nightly fmt --check`, `just build-fw-esp32`

**Validate**: All commands pass. No warnings. Plan file moved to docs/plans-done/. Commit with
Conventional Commits.

---

## Notes

**Q1–Q4 answers**: See Design section. BuiltinId in lps-builtin-ids; format() in cranelift via
extension trait; gen-app emits two outputs; DEFAULT_MAX_ERRORS in frontend.

**format() implementation**: Rust does not allow adding inherent methods to types from other crates.
Use an extension trait in lps-cranelift:
`trait BuiltinIdFormat { fn format(&self) -> Option<DecimalFormat>; } impl BuiltinIdFormat for lps_builtin_ids::BuiltinId { ... }`.
Callers `use crate::backend::builtins::BuiltinIdFormat;` then call `id.format()`.
