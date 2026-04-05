# Stage I: Builtin Naming Convention

## Goal

Establish `__lp_<module>_<fn>_<mode>` as the universal naming convention
for all builtins. Rename all existing symbols, update all consumers,
make BuiltinId self-describing.

## Suggested plan name

`lpir-cranelift-stage-i`

## Scope

**In scope:**

- Redesign `BuiltinId` in `lps-builtin-ids` to be self-describing:
  given (module, name, mode) it derives symbol name, LPIR import path,
  file path. Replace the flat generated enum (`LpQ32Sin`, `LpfxFbm2F32`,
  etc.) with a structured representation.
- Rename all builtin symbols:
    - `__lp_q32_sin` → `__lps_sin_q32`
    - `__lpfx_fbm2_f32` → `__lp_lpfx_fbm2_f32`
    - `__lp_q32_add` (etc.) → `__lp_lpir_fadd_q32` (intrinsic math)
    - Mode-independent functions (hash) get no suffix
- Update `lps-builtins`: rename `#[no_mangle]` symbols
- Update `lps-builtins-gen-app`: generate new naming
- Update WASM emitter import resolution (`lps-wasm/src/emit/imports.rs`)
- Update Cranelift backend builtin declaration/mapping (old crate still
  needs to compile on main — or we accept breakage on branch)
- Update LPIR import module names: `std.math` → `glsl`
- Update `lps-naga` lowering: register imports as `glsl::sin` etc.
- Update `StdMathHandler` and any test import handlers
- Update `lps-filetests` if it references symbol names
- All existing tests pass after rename

**Out of scope:**

- New crate (Stage II)
- File path reorganization in lps-builtins (nice to have, can be
  deferred — the symbol rename is the critical part)
- Adding new builtins

## Key decisions

- Three modules: `lpir` (IR ops needing library support), `glsl` (GLSL
  std functions), `lpfx` (LightPlayer effects)
- Mode suffix `_q32` / `_f32` for float-mode-specific, no suffix for
  mode-independent
- `BuiltinId` should support: `symbol() -> &str`, `module() -> Module`,
  `name() -> &str`, `mode() -> Option<Mode>`
- The `glsl_q32_math_builtin_id` and `glsl_lpfx_q32_builtin_id` mapping
  functions in `lps-builtin-ids` should be updated to work with the
  new naming. These become the shared import resolution used by both WASM
  and Cranelift emitters.

## Open questions

- **BuiltinId representation**: Should it remain an enum (with structured
  derives) or become a struct `{ module, name, mode }`? Enum is better for
  exhaustive matching and known-at-compile-time sets. Struct is more
  flexible. Likely enum with derive macros, but worth considering.
- **Generator changes**: The builtins generator (`lps-builtins-gen-app`)
  currently generates `registry.rs`, `mapping.rs`, `builtin_refs.rs` for
  multiple consumers. The rename may simplify some of this (if BuiltinId
  is self-describing, less generated mapping code is needed). Worth
  understanding the generator before committing to implementation approach.
- **Branch strategy**: The old `lps-cranelift` still compiles on this
  branch (we haven't deleted it yet). Do we update its builtin references
  too, or accept that it's broken? Since we're abandoning it, breaking it
  is fine — but tests that exercise the old path will fail. May want to
  `#[cfg]`-gate or just accept red tests for the old backend.
- **LPIR text format**: If filetests or LPIR text files reference
  `std.math::sin`, they need updating to `glsl::sin`. Check if the parser
  and printer need changes.

## Deliverables

- Restructured `BuiltinId` with self-describing API
- All `__lp_*` symbols renamed across `lps-builtins`
- Updated generator
- LPIR imports use `glsl::` and `lpfx::` module names
- WASM emitter import resolution updated
- All WASM-path and LPIR-path tests passing

## Dependencies

None — this is the first stage.

## Estimated scope

~500 lines changed across 8–10 files. Mostly mechanical search-and-replace
with some structural work on `BuiltinId`.
