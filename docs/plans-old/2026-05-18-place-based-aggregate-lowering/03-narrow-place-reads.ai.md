# Phase 3: Narrow Place Reads

## Scope of phase

Use the same place-lowering model for reads so addressable aggregate reads do not load the whole root unnecessarily.

In scope:

- Add `lower/place/read.rs`.
- Route `read_assign_target` through lowered-place reads where possible.
- Keep existing `read_segments` fallback for unsupported dynamic flat paths.
- Add tests for reading constant-index struct fields and vector swizzles.

Out of scope:

- Dynamic memory indexing if phase 4 has not landed yet.
- Removing all old projection helpers.

## Code organization reminders

- Prefer granular files with one main concept per file.
- Keep helpers lower in the file.
- Put tests at the bottom.
- Mark temporary code with a clear `TODO` only if truly unavoidable.

## Sub-agent reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation details

Relevant files:

- `lp-shader/lps-glsl/src/lower/place/read.rs`
- `lp-shader/lps-glsl/src/lower/ops/place_read.rs`
- `lp-shader/lps-glsl/src/lower/ops/place_project.rs`

Expected changes:

- `read_assign_target` should try `lower_place_for_read` first.
- Memory-backed constant leaves should emit leaf-width loads.
- Flat-lane leaves should reuse existing vregs without copies.
- Unsupported cases should keep current behavior.

## Validate

```bash
cargo fmt --check
cargo test -p lps-glsl place -- --nocapture
cargo test -p lp-shader compute -- --nocapture
```
