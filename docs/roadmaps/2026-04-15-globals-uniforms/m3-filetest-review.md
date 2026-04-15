# Milestone 3: Globals & Uniforms Filetest Review

## Goal

Audit, clean up, and extend the `filetests/global/` test suite to match the
concrete architecture from M1–M2. Remove duplicates, fix misleading names,
add missing coverage for the init/reset lifecycle, and clearly scope what's
in vs out of this roadmap.

## Suggested Plan Name

`globals-uniforms-m3`

## Scope

### Directory restructure

The current `filetests/global/` mixes private globals, uniforms, `in`, `out`,
`buffer`, `shared`, and `const` in one flat directory. Restructure into:

```
filetests/
├── global/              ← private globals (no qualifier), in scope
├── uniform/             ← NEW directory, in scope
├── global-future/       ← out-of-scope qualifiers (in/out/buffer/shared)
```

- **`global/`**: Private (unqualified) globals and `const` globals. This is
  the core of the roadmap. `const` globals are resolved by Naga at compile
  time (literal values, no vmctx storage) — they should work without globals
  infrastructure and don't need `@unimplemented` tags.

- **`uniform/`**: New directory for uniform-specific tests. Move existing
  uniform tests here from `global/` and add new ones.

- **`global-future/`**: Move `in`, `out`, `buffer`, `shared` tests here.
  These qualifiers are out of scope for this roadmap but the tests document
  intent for future work. Keep them `@unimplemented`.

### Audit and cleanup

- **Remove duplicates**: `initialize-undefined.glsl` and
  `edge-uninitialized-read.glsl` test the same thing. Merge or delete one.
  Similarly, `declare-uniform.glsl`, `uniform-readonly.glsl`,
  `uniform-no-init-error.glsl`, and `initialize-uniform.glsl` have heavy
  overlap — consolidate into the new `uniform/` directory as a focused set.

- **Fix misleading names**: `shared-array-size.glsl` uses `uniform` arrays,
  not `shared` — move to `uniform/`. `access-from-main.glsl` doesn't define
  `main` — rename. `shared-globals.glsl` and `shared-struct-match.glsl` use
  `uniform` — move to `uniform/`.

- **Sort files into directories**: Each existing file goes to `global/`,
  `uniform/`, or `global-future/` based on what qualifier it primarily tests.
  Files that mix qualifiers should be split or reorganized.

### New tests needed

**Lifecycle tests** (new category: `filetests/global/lifecycle/` or inline):

- **Reset between calls**: A function that increments a global and returns it.
  Called twice with `// run:` — both calls should return the same value
  (proving globals reset between calls).
  ```
  float counter = 0.0;
  float test_reset() { counter += 1.0; return counter; }
  // run: test_reset() ~= 1.0
  // run: test_reset() ~= 1.0   ← same result, globals reset
  ```

- **Multiple globals reset**: Several globals of different types, mutated in
  one call, verified as reset in the next.

- **No-globals shader**: A shader with no globals and no uniforms. Verifies
  the fast path (zero-overhead, no init/reset). Should already work today.

- **Uniforms-only shader**: Uniforms but no mutable globals. Verifies no
  snapshot/reset is needed, only uniform reads.

**Uniform-dependent initializers** (new tests):

- **Simple uniform init**: `float x = uniform_time * 2.0;` with
  `// set_uniform: time = 3.0` → `x` should be `6.0`.

- **Multi-uniform init**: Global initialized from multiple uniforms.

- **Uniform init + mutation**: Global initialized from uniform, then mutated
  in the function, verified as reset on next call.

**Non-zero uniform tests** (extend existing):

- **`// set_uniform:` with values**: Existing uniform filetests all assume
  zero defaults. Add tests with `// set_uniform: time = 1.0` etc. and verify
  non-zero results.

- **Uniform struct**: `uniform MyStruct { ... }` with `// set_uniform:` for
  struct fields (if struct uniforms are in scope — may defer).

**Layout/alignment tests**:

- **Mixed-type globals**: `float`, `vec3`, `float`, `vec4` — verifies std430
  alignment padding is correct (vec3 followed by float, then vec4 at 16-byte
  boundary).

- **Struct global**: Global of struct type, field access and mutation.

- **Array global**: Global array, element access by index.

**Cross-function tests**:

- **Helper mutates global**: Function `a()` calls `b()` which mutates a
  global; `a()` reads it back. Verifies globals are shared across the call
  tree within one invocation.

- **Init calls helper**: Global initializer expression calls a user function
  (if Naga supports this — may be constant-expression-only).

### Out of scope

- `in`/`out`/`buffer`/`shared` qualifier support (tag existing tests, don't
  implement).
- Compile-error assertion infrastructure (the `edge-*-error` files use
  comments for illegal code but don't assert parse failures — improving this
  is separate work).

## Key Decisions

- This milestone is about **test quality**, not implementation. M1 and M2
  provide the implementation; M3 ensures the tests match the architecture.

- Tests should be written so they can be un-gated incrementally as M1/M2
  deliver functionality. New tests start with `@unimplemented` on all
  backends and get un-gated as the implementation lands.

- The `// set_uniform:` directive syntax is defined here but implemented in
  the filetest runner as part of M4 (engine + filetest integration).

## Deliverables

- `filetests/global/` — private globals only (cleaned up, deduped).
- `filetests/uniform/` — new directory with focused uniform tests.
- `filetests/global-future/` — `in`/`out`/`buffer`/`shared` tests moved here.
- New lifecycle tests (reset, no-globals, uniforms-only).
- New uniform-dependent initializer tests.
- New non-zero uniform tests using `// set_uniform:`.
- New layout/alignment tests.

## Dependencies

- M1 and M2 should be in progress or complete so we know the exact
  architecture. However, test files can be written with `@unimplemented`
  before the implementation lands.

## Estimated Scope

~20-30 test files created/modified. No Rust code changes.

## Agent Execution Notes

This milestone should be done by the orchestrating agent (Opus), not a Kimi
sub-agent, since it requires architectural judgment about what to test and
how tests map to the implementation design.

Steps:
1. Read all existing `filetests/global/` files.
2. Identify duplicates and merge/delete.
3. Rename misleading files.
4. Write new lifecycle, uniform-dependent, layout, and cross-function tests.
5. Tag out-of-scope tests clearly.
6. Verify `const` tests pass without globals infrastructure.
