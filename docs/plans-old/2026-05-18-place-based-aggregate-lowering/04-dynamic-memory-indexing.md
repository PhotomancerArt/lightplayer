# Phase 4: Dynamic Memory Indexing

## Scope of phase

Implement dynamic indexing for memory-backed aggregate places.

In scope:

- Add dynamic byte-offset calculation for array index segments.
- Support reads and writes through `base + index * stride + field_offset`.
- Preserve current bounds/clamping policy unless a test proves it wrong.
- Add dynamic-index tests for memory-backed arrays.

Out of scope:

- Dynamic flat-register optimization beyond existing fallback.
- Changing user-visible GLSL semantics unless required for correctness.

## Code organization reminders

- Prefer granular files with one main concept per file.
- Put dynamic helpers in `lower/place/dynamic.rs`.
- Keep tests at the bottom.
- Mark temporary code with a clear `TODO` only if truly unavoidable.

## Sub-agent reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation details

Relevant files:

- `lp-shader/lps-glsl/src/lower/place/dynamic.rs`
- `lp-shader/lps-glsl/src/lower/place/path.rs`
- `lp-shader/lps-glsl/src/lower/place/read.rs`
- `lp-shader/lps-glsl/src/lower/place/write.rs`
- `lp-shader/lps-glsl/src/lower/ops/index.rs`

Expected changes:

- For dynamic array indices into memory-backed places, compute `index * stride`.
- Add the dynamic offset to any static field offsets.
- Emit narrow loads/stores at the computed leaf address.
- Keep flat-register dynamic indices on the existing select/merge fallback.

Important edge cases:

- Negative indices currently clamp in `lower_index`; dynamic memory indexing should preserve or deliberately document equivalent behavior.
- Offsets must fit LPIR immediate forms or use pointer arithmetic vregs.

## Validate

```bash
cargo fmt --check
cargo test -p lps-glsl dynamic -- --nocapture
cargo test -p lp-shader compute -- --nocapture
```
