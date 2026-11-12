# Plan D: Open Questions — Resolved

## Q1: Both paths or Q32 only?
**Decision**: Replace the transform path. Filetests validate correctness.

## Q2: Streaming path — simplify now or later?
**Decision**: Simplify now in Plan D. Single module, no transform.

## Q3: Object module (emulator) path
**Decision**: Include in Plan D for consistency.

## Q4: Signature building
**Decision**: Option (b) — make SignatureBuilder numeric-aware. Less
wasteful, more correct.

## Q5: compile_single_function_to_clif — NumericMode parameter
**Decision**: Option (a) — add `numeric_mode: NumericMode` parameter.
Explicit, no state on GlslCompiler.

## Q6: Feature flag / config toggle
**Decision**: No feature flag. Replace the path directly.

## Q7: Builtins declaration
**Decision**: Existing `declare_builtins()` is sufficient. No changes.
