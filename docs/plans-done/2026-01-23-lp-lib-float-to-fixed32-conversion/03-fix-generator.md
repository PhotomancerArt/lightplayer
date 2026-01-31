# Phase 3: Fix Generator to Use LpLibFn as Source of Truth

## Goal

Update `lp-glsl-builtin-gen-app` to use `LpLibFn` enum as the source of truth instead of prefix
matching. The generator should read the enum to know what functions exist, then match discovered
functions to expected names.

## Tasks

### 3.1 Read LpLibFn Enum in Generator

In `lp-glsl/lp-glsl-builtin-gen-app/src/main.rs`:

- Import or parse `LpLibFn` enum from `lp-glsl-compiler` crate
- Iterate over all `LpLibFn` variants to know what functions should exist
- For each variant, determine expected function name:
    - Use `q32_name()` if it returns `Some(_)` (simplex functions)
    - Use `symbol_name()` if `q32_name()` returns `None` (hash functions)

### 3.2 Update Function Discovery

Modify `extract_builtin()` or discovery logic:

- Match discovered function names against expected names from `LpLibFn`
- Use `LpLibFn::builtin_id()` to determine `BuiltinId` variant name (e.g., `LpSimplex3`, not
  `Q32LpSimplex3`)
- Generate `BuiltinInfo` with correct enum variant name

### 3.3 Update Registry Generation

Ensure `generate_registry()`:

- Uses `LpLibFn::builtin_id()` to get correct `BuiltinId` variant names
- Maps `BuiltinId::LpSimplex3.name()` to actual function name (`__lp_q32_lpfx_snoise3`)
- Generates correct enum variants matching what `lp_lib_fns.rs` expects

### 3.4 Update TestCase Mapping Generation

Ensure `generate_testcase_mapping()`:

- Uses `LpLibFn::symbol_name()` for TestCase names
- Maps to correct `BuiltinId` variants from `LpLibFn::builtin_id()`

## Success Criteria

- Generator reads `LpLibFn` enum instead of using prefix matching
- Generated registry has `LpSimplex1/2/3` variants (not `Q32LpSimplex1/2/3`)
- `BuiltinId::LpSimplex3.name()` returns `"__lp_q32_lpfx_snoise3"`
- Code compiles without warnings
- Code formatted with `cargo +nightly fmt`

## Code Organization

- Place helper utility functions **at the bottom** of files
- Place more abstract things, entry points, and tests **first**
- Keep related functionality grouped together

## Formatting

- Run `cargo +nightly fmt` on all changes before committing
- Ensure consistent formatting across modified files

## Language and Tone

- Keep language professional and restrained
- Avoid overly optimistic language like "comprehensive", "fully production ready", "complete
  solution"
- Avoid emoticons
- Code is never done, never perfect, never fully ready, never fully complete
- Use measured, factual descriptions of what was implemented
