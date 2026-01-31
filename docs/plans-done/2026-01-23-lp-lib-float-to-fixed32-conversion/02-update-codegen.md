# Phase 2: Update Codegen to Emit TestCase Calls

## Goal

Change `emit_lp_lib_fn_call()` to emit TestCase calls for functions that need q32 mapping, instead
of directly calling builtins. This allows the q32 transform to handle the conversion.

## Tasks

### 2.1 Update `emit_lp_lib_fn_call()` Logic

In `lp-glsl/lp-glsl-compiler/src/frontend/codegen/lp_lib_fns.rs`:

- Check `needs_q32_mapping()` to determine if function needs TestCase conversion
- If `true` (simplex functions):
    - Get TestCase name from `symbol_name()` (e.g., `"__lpfx_snoise3"`)
    - Flatten vector arguments (already done)
    - Emit TestCase call using pattern similar to `get_math_libcall()`
    - Create signature with `F32` types (before transform)
- If `false` (hash functions):
    - Keep existing direct builtin call behavior (no change needed)

### 2.2 Create Helper for TestCase Calls

Create helper method similar to `get_math_libcall()`:

-
`get_lp_lib_testcase_call(&mut self, lp_fn: LpLibFn, arg_count: usize) -> Result<FuncRef, GlslError>`
- Use `lp_fn.symbol_name()` to get TestCase name
- Create signature with `F32` params/returns (before transform)
- Return `FuncRef` for the TestCase call

### 2.3 Handle Different Argument Counts

Ensure the helper correctly handles:

- Simplex1: 2 args (i32, u32) → TestCase call with 2 F32 params
- Simplex2: 3 args (i32, i32, u32) → TestCase call with 3 F32 params
- Simplex3: 4 args (i32, i32, i32, u32) → TestCase call with 4 F32 params

Note: Vector arguments are already flattened before this point, so we're working with scalar counts.

## Success Criteria

- Simplex functions emit TestCase calls (not direct builtin calls)
- Hash functions continue to use direct builtin calls
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
