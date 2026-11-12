# Milestone 4: Engine Integration + Filetest Infrastructure

## Goal

The engine render loop handles the full globals/uniforms lifecycle. Filetests
support `// set_uniform:` directives for uniform-dependent tests. All existing
global filetests are un-gated and passing.

## Suggested Plan Name

`globals-uniforms-m4`

## Scope

### In scope

- **Engine render loop** (`lp-engine/src/gfx/native_jit.rs`,
  `lp-engine/src/gfx/cranelift.rs`): Update `render_native_jit_direct` and
  the Cranelift equivalent to:
  1. Set built-in uniforms (time, resolution) on the instance.
  2. Call `__shader_init` via direct call.
  3. Memcpy globals → snapshot.
  4. Per-pixel loop: memcpy snapshot → globals (if `globals_size > 0`),
     then `call_direct`.
  5. Optimize: skip init/reset entirely when `globals_size == 0` and
     `uniforms_size == 0` (pure shader fast path unchanged from today).

- **`LpShader` trait updates**: If needed, add `set_uniform` or pass uniform
  values through the `render` method signature. The engine currently passes
  `time` as a render argument — uniforms like `time` and `resolution` could
  either stay as function args or move to the uniform region. Design decision
  at implementation time.

- **Filetest `// set_uniform:` directive**: New syntax in the filetest runner:
  ```
  // set_uniform: time = 1.0
  // set_uniform: resolution = vec2(800.0, 600.0)
  // run: test_func() ~= 42.0
  ```
  The runner parses these, calls `set_uniform` on the instance before
  `init_globals` + the test call.

- **Per-test globals reset**: Each `// run:` line gets fresh globals. The
  runner calls `init_globals()` (which re-runs `__shader_init` and
  re-snapshots) before each test, ensuring isolation. If `// set_uniform:`
  lines precede a `// run:`, they're applied before init.

- **Un-gate remaining filetests**: Remove `@unimplemented` from all global
  filetests including uniform-dependent ones. Fix any that fail.

- **Uniform filetests**: The existing `global/declare-uniform.glsl` and
  `global/access-read.glsl` (which use uniforms) should pass with the new
  `// set_uniform:` directive or with default-zero uniform values.

### Out of scope

- `in`/`out`/`buffer`/`shared` qualifiers (future work).
- Inline tight loop optimization (M5 future work).
- Non-Q32 float modes for uniforms.

## Key Decisions

- The engine fast path (`render_native_jit_direct`) handles init/reset
  internally. The `LpShader` trait doesn't expose init/reset — it's an
  implementation detail of each backend's render function.

- Built-in uniforms (time, resolution) can be set by the engine before
  calling render. Whether these are passed as function args (current) or
  through the uniform region is a design choice for implementation. Both
  can coexist — function args for built-in "magic" uniforms, uniform region
  for user-declared uniforms.

- The `globals_size == 0` optimization means existing shaders with no globals
  see zero overhead. The hot path is identical to today's code.

## Deliverables

- Updated `lp-engine/src/gfx/native_jit.rs` — render loop with
  init/reset lifecycle.
- Updated `lp-engine/src/gfx/cranelift.rs` — same.
- Updated filetest runner — `// set_uniform:` parsing, per-test init/reset.
- All `filetests/global/` tests un-gated and passing.
- New uniform-dependent filetests if needed.

## Dependencies

- M2 (instance lifecycle): `set_uniform`, `init_globals`, `reset_globals`
  working on all backends.
- M3 (filetest review): cleaned up and extended test suite ready to un-gate.

## Estimated Scope

~200-400 lines engine changes, ~150-300 lines filetest runner changes,
~50-100 lines filetest updates.

## Agent Execution Notes

This milestone may benefit from two sequential agent sessions:

**Session 1 — Filetest infrastructure**:
1. Read the filetest runner (`lps-filetests/src/test_run/`) to understand
   how `// run:` lines are parsed and executed.
2. Add `// set_uniform:` parsing.
3. Wire per-test `init_globals` + `reset_globals` into the runner.
4. Un-gate and run `global/declare-simple.glsl` first, then expand.

**Session 2 — Engine integration**:
1. Read `lp-engine/src/gfx/native_jit.rs` and `cranelift.rs`.
2. Add init/reset to the render loop.
3. Test with a shader that uses globals and verify correct rendering.
4. Verify `globals_size == 0` path is unchanged.

Verify:
- `cargo test -p lps-filetests` — all global tests pass.
- `cargo test -p fw-tests --test scene_render_emu` — existing render tests
  still pass.
- `cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf
  --profile release-esp32 --features esp32c6,server` — firmware builds.
