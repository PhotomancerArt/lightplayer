# Phase 5: Fallback Dynamic Flat Path

## Scope of phase

Clarify and constrain the old select/rebuild path so it is explicitly a fallback for non-addressable dynamic flat values.

In scope:

- Rename or wrap `assign_index_value` usage to make fallback intent clear.
- Add tests proving dynamic vector/matrix writes still work.
- Ensure constant-index addressable places do not use fallback.

Out of scope:

- A general LPIR optimization pass.
- Removing every old helper if still needed by fallbacks.

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

- `lp-shader/lps-glsl/src/lower/ops/index.rs`
- `lp-shader/lps-glsl/src/lower/ops/place_project.rs`
- `lp-shader/lps-glsl/src/lower/place/dynamic.rs`

Expected changes:

- Existing select/rebuild code remains available only through fallback paths.
- Naming and comments should make it clear why fallback is used.
- Tests should prevent regressions in dynamic vector/matrix assignment.

## Validate

```bash
cargo fmt --check
cargo test -p lps-glsl index -- --nocapture
cargo test -p lp-shader compile_px_desc_lps_glsl_basic_shader -- --nocapture
```
