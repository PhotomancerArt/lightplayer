# Stage VII: Cleanup — Delete Old Compiler

## Goal

Remove the old `lps-cranelift` crate, the `lps-frontend` crate
(if no longer needed), and any dead code from the migration. Verify
everything still builds and passes.

## Suggested plan name

`lpvm-cranelift-stage-vii`

## Scope

**In scope:**

- Delete `lp-shader/lps-cranelift/` (the old AST→CLIF compiler)
- Delete `lp-shader/lps-frontend/` (the old GLSL frontend/parser) if
  nothing else depends on it
- Delete the `glsl` crate dependency chain if unused (the old hand-written
  GLSL parser)
- Remove `cranelift.q32` from filetest targets if still present (typically
  already removed in **Stage V2**; replaced by `jit.q32` and `rv32.q32`)
- Remove any compatibility shims, `#[allow(dead_code)]` annotations, or
  conditional compilation added during migration
- Update old builtins generator code if it still references old naming
- Remove old `map_testcase_to_builtin` and any testcase-name mapping
  that only existed for the old compiler's float-mode linking
- Update workspace Cargo.toml to remove old crate entries
- Verify: `cargo build`, `cargo test`, filetests, fw-esp32 build all pass
- Update any documentation that references the old crate or API

**Out of scope:**

- New features
- Optimizations
- LPIR vector support

## Key decisions

- This is a deletion-heavy stage. The goal is a clean codebase with no
  remnants of the old compiler.
- The `glsl` crate (Rust GLSL parser) was only used by `lps-frontend`.
  If nothing else uses it, it can be removed from the workspace.
- Filetests that were annotated with `// @ignore(backend=jit)` or
  `// @unimplemented(backend=jit)` should be reviewed — some may now be
  fixable, others are genuine future work.

## Open questions

- **`lps-frontend` dependents**: Does anything besides the old
  `lps-cranelift` depend on `lps-frontend`? If so, those need
  migration first. Check workspace dependency graph.
- **`lps-builtins-gen-app`**: The generator creates code for both old
  and new builtins infrastructure. After deleting the old crate, simplify
  the generator to only produce what the new system needs. This may be a
  meaningful simplification (remove `registry.rs`, `mapping.rs` generation
  for the old Cranelift backend).
- **Example code**: `lps-cranelift/examples/simple.rs` — anything to
  preserve or replace with a new-crate example?
- **Integration tests**: `lps-cranelift/tests/` — review if any test
  logic should be ported to the new crate's tests before deletion.
- **Web demo**: Does the web demo use the old compiler path at all, or is
  it purely WASM? If WASM-only, unaffected.

## Deliverables

- Old compiler crate deleted
- Old frontend crate deleted (if safe)
- Clean workspace: no dead code, no unused dependencies
- All tests, filetests, and firmware builds passing
- Updated documentation

## Dependencies

- Stage VI-C (ESP32 hardware validation) must be complete and stable.
  No point deleting the old compiler until we're confident in the
  replacement across desktop, emulator, and hardware.

## Estimated scope

~2000+ lines deleted. ~100 lines of Cargo.toml / build system updates.
Documentation updates. The effort is mostly verification — making sure
nothing breaks.

## Temporarily ignored tests (re-enable during cleanup)

These were ignored while LPIR / builtins / WASM linking is in flux. Before
closing Stage VII, remove `#[ignore]`, run the tests, and fix any failures.

| Location                                                                                                           | What                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
|--------------------------------------------------------------------------------------------------------------------|------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `lp-shader/lps-filetests/tests/lpfn_builtins_memory.rs` — `shader_lpfn_saturate_vec3_writes_scratch_then_reads_it` | Links shader WASM with `lps_builtins_wasm.wasm`. Failed with `incompatible import type for builtins::__lpfn_saturate_vec3_q32`: Naga/LPIR lowers `vec3 lpfn_saturate(vec3)` as a WASM import with three i32 params and three i32 results, while the Rust builtin is `(result_ptr: i32, x, y, z) -> ()`. **Re-check:** align LPIR import + `lps-wasm` emission with the result-pointer ABI (or document an adapter), then un-ignore and verify scratch-memory behavior. |
