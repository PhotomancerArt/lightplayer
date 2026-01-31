# Phase 5: Verify Transform Mappings

## Goal

Verify that `map_testcase_to_builtin()` correctly maps TestCase names to `BuiltinId` variants, and
that the transform correctly converts TestCase calls to q32 builtin calls.

## Tasks

### 5.1 Verify TestCase Mapping

Check `lp-glsl/lp-glsl-compiler/src/backend/transform/q32/converters/math.rs`:

- `map_testcase_to_builtin("__lpfx_snoise1")` should return `Some((BuiltinId::LpSimplex1, 2))`
- `map_testcase_to_builtin("__lpfx_snoise2")` should return `Some((BuiltinId::LpSimplex2, 3))`
- `map_testcase_to_builtin("__lpfx_snoise3")` should return `Some((BuiltinId::LpSimplex3, 4))`
- Hash functions should not be in the mapping (they don't go through transform)

### 5.2 Verify Transform Conversion Logic

Check `lp-glsl/lp-glsl-compiler/src/backend/transform/q32/converters/calls.rs`:

- `convert_call()` should detect TestCase calls to `"__lpfx_snoise3"`
- Should map to `BuiltinId::LpSimplex3` via `map_testcase_to_builtin()`
- Should use `BuiltinId::LpSimplex3.name()` to get `"__lp_q32_lpfx_snoise3"`
- Should look up `"__lp_q32_lpfx_snoise3"` in `func_id_map`
- Should create call to the q32 function

### 5.3 Add Tests if Needed

If transform logic needs verification:

- Add test cases for TestCase → builtin conversion
- Verify argument mapping (F32 → I32)
- Verify function name resolution

## Success Criteria

- `map_testcase_to_builtin()` correctly maps simplex TestCase names
- Transform correctly converts TestCase calls to q32 builtin calls
- All code compiles without warnings
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
