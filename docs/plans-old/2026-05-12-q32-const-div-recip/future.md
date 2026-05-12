# Future Work

## Debug Math Probes

- **Idea:** Add probe/instrumentation modes that detect overflow, div-zero, and fixed-point range hazards during shader debugging.
- **Why not now:** This pass is focused on the normal render hot path and should not mix debug semantics into product lowering.
- **Useful context:** The old Q32 saturating/reference helpers remain useful as probe/reference behavior.

## Dynamic Inline Reciprocal Div

- **Idea:** Inline the general dynamic `Fdiv` reciprocal path in `lpvm-native`.
- **Why not now:** The previous attempt failed filetests; it needs a correctness-first backend implementation.
- **Useful context:** `docs/reports/2026-05-12-jit-math-perf.md` records the failed attempt and hardware cycle data.

## LPIR Constant Propagation For Derived Constants

- **Idea:** Add a config-aware or semantic LPIR pass that rewrites derived constant divisors into `FdivConstF32`.
- **Why not now:** Frontend emission should capture the easy shader cases first with less machinery.
- **Useful context:** `lpir::const_fold` currently only tracks integer constants.

## Remove Public Math Mode Surface

- **Idea:** Fully delete user-facing Q32 add/mul/div mode slots and `compile-opt(q32.*)` once fast-only product semantics are settled.
- **Why not now:** Public schema/UI churn may distract from the const-div perf work.
- **Useful context:** `lpc-model::GlslOpts`, `lpir::CompilerConfig::q32`, and backend mode branches are the main cleanup surface.
