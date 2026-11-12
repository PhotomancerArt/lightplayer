# Phase 1: Layout and Constant Path Analysis

## Scope of phase

Add the small `lps-glsl` place-layout foundation needed to reason about constant-index paths without materializing whole aggregate roots.

In scope:

- Add `lp-shader/lps-glsl/src/lower/place/` module skeleton.
- Add `layout.rs` helpers around `LpsType` for lane count, array stride, field offsets, and constant index extraction.
- Add tests for `FluidEmitter[4]`-style lane and byte layout.

Out of scope:

- Rewriting assignment lowering.
- Dynamic memory indexing implementation.
- Naga frontend changes.

## Code organization reminders

- Prefer granular files with one main concept per file.
- Keep helpers lower in the file when that improves readability.
- Put tests at the bottom of each file.
- Mark temporary code with a clear `TODO` only if truly unavoidable.

## Sub-agent reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation details

Relevant files:

- `lp-shader/lps-glsl/src/lower.rs`
- `lp-shader/lps-glsl/src/lower/place/mod.rs`
- `lp-shader/lps-glsl/src/lower/place/layout.rs`
- `lp-shader/lps-glsl/src/hir/place.rs`

Expected changes:

- Declare `mod place;` from `lower.rs`.
- Add layout helpers using `lps_shared::{array_stride, type_size, LayoutRules, LpsType}`.
- Add a helper for extracting `usize` constants from `HirExprKind::{IntLiteral, UIntLiteral}`.
- Add a helper to project a lane range through constant array indexes and field segments.

Important edge cases:

- Negative integer indices should not be treated as valid constant indexes.
- Out-of-range constant indexes should return `None` or an error, not wrap.
- Layout helpers should stay `no_std + alloc` friendly.

## Validate

```bash
cargo fmt --check
cargo test -p lps-glsl place -- --nocapture
```
