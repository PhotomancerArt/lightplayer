# Phase 6: Migrate Hand-Written Test Files

## Scope

Update all ~634 hand-written `.glsl` test files to the new annotation format.
This is a bulk mechanical transformation.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### Transformation Rules

For every `.glsl` file in `lp-shader/lp-glsl-filetests/filetests/` (excluding
`.gen.glsl` files, which are handled in phase 7):

1. **Remove `// target riscv32.q32`** line (and any `// target wasm32.q32`)
2. **Convert `[expect-fail]`** suffix on `// run:` lines:
    - `// run: expr() == value [expect-fail]`
    - becomes two lines:
      ```
      // @unimplemented()
      // run: expr() == value
      ```
3. **Add file-level `@unimplemented(backend=wasm)`** for files that use
   features not supported by the wasm backend. This covers the vast majority
   of test files since wasm currently only supports scalar arithmetic.

   Files that should NOT get this annotation (they should work on wasm):
    - `scalar/int/op-add.glsl` and similar basic scalar int arithmetic
    - `scalar/int/op-subtract.glsl`
    - `scalar/int/op-unary-minus.glsl`
    - `scalar/float/op-add.glsl` and similar basic scalar float arithmetic
    - `scalar/bool/` basic tests (literals, comparisons)
    - Any file that only uses: int/float/bool scalars, `+`, `-`, `*` (int
      only), comparisons, variable declarations, return statements, function
      params

   Files that SHOULD get `@unimplemented(backend=wasm)`:
    - Everything in `vec/`, `matrix/`, `struct/`, `array/`, `global/`
    - Everything in `builtins/`
    - Everything in `control/` (if/else, loops — wasm doesn't support these)
    - Everything in `function/` (multi-function calls, out/inout params)
    - Everything in `operators/` (inc/dec on vectors/matrices)
    - Everything in `lpfx/`
    - Anything using vectors, matrices, structs, arrays, globals, builtins,
      control flow, or multi-function calls

4. **Move `wasm/int-add.glsl`** to `scalar/int/` (or just delete it if
   `scalar/int/op-add.glsl` already covers the same cases).

### Approach

Write a small script or use the agent to do the transformation. The steps
are:

1. For all `.glsl` files (not `.gen.glsl`):
    - Read file
    - Remove `// target ...` line
    - For each `// run:` line ending in `[expect-fail]`:
        - Remove `[expect-fail]` from the line
        - Insert `// @unimplemented()` on the line before it
    - Write file

2. Determine which files need `@unimplemented(backend=wasm)`:
    - Run `--target wasm.q32` and collect all failures
    - Or categorize by directory (safer: all non-scalar-basic dirs)
    - Add `// @unimplemented(backend=wasm)` after the `// test run` line

3. Handle the `wasm/int-add.glsl` move.

### Verification

After migration, run:

```bash
# All tests should pass on cranelift
scripts/glsl-filetests.sh --target cranelift.q32

# Wasm tests: scalar basics should pass, everything else should be
# expected-failure (from @unimplemented annotations)
scripts/glsl-filetests.sh --target wasm.q32
```

No test file should have `// target` or `[expect-fail]` remaining:

```bash
grep -r '// target' lp-shader/lp-glsl-filetests/filetests/ --include='*.glsl' | grep -v '.gen.glsl'
grep -r '\[expect-fail\]' lp-shader/lp-glsl-filetests/filetests/ --include='*.glsl' | grep -v '.gen.glsl'
```

Both greps should return empty.

## Validate

```
cargo test -p lp-glsl-filetests
scripts/glsl-filetests.sh
```

All tests should pass (including expected failures counting correctly).
