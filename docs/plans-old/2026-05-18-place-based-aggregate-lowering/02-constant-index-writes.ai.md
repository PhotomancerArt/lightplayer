# Phase 2: Constant-Index Writes

## Scope of phase

Lower constant-index aggregate writes directly for flat and memory-backed places.

In scope:

- Add place write classification for constant index, field, and swizzle paths.
- Make `emitters[0].pos = vec2(...)` avoid `assign_index_value`.
- Emit narrow copies/stores for exact leaf lanes/offsets.
- Add regression tests for array-of-struct constant field writes.

Out of scope:

- Dynamic memory indexing.
- Read-path rewrite.
- LPIR optimization passes.

## Code organization reminders

- Prefer granular files with one main concept per file.
- Keep related functionality grouped in `lower/place/`.
- Put tests at the bottom of the file they exercise.
- Mark temporary code with a clear `TODO` only if truly unavoidable.

## Sub-agent reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation details

Relevant files:

- `lp-shader/lps-glsl/src/lower/place/path.rs`
- `lp-shader/lps-glsl/src/lower/place/write.rs`
- `lp-shader/lps-glsl/src/lower/ops/place_write.rs`
- `lp-shader/lps-glsl/src/lower/ops/place_project.rs`
- `lp-shader/lps-glsl/src/lower/storage.rs`
- `lp-shader/lp-shader/src/tests.rs`

Expected changes:

- Introduce `lower_place_for_write` for constant-addressable paths.
- For flat local/param values with constant path lanes, copy only selected lanes.
- For pointer params, slot-backed locals, and globals, compute base plus static byte offset and store only selected leaf lanes.
- Preserve existing whole-value fallback for unsupported paths.
- Add a test that compiles a `FluidEmitter emitters[4]` compute shader and checks the output values.
- Add a shape test that constant indexed writes emit no whole-array select chain.

Important edge cases:

- Global roots use VMContext plus the root byte offset.
- Slot-backed locals use the slot address as base.
- Produced compute outputs are global roots and should benefit from this phase.

## Validate

```bash
cargo fmt --check
cargo test -p lp-shader compile_compute_reads_struct_array_output -- --nocapture
cargo test -p lps-glsl constant -- --nocapture
```
