# Summary

## What was built

- Added a place-lowering layer in `lps-glsl` for direct flat-lane and memory-backed aggregate access.
- Lowered constant-index aggregate writes like `emitters[0].pos` to narrow stores/copies instead of whole-root rebuilds.
- Added dynamic memory indexing for addressable aggregate arrays like `emitters[selected].pos`.
- Routed addressable place reads through the same narrow lowering path before falling back to whole-root projection.
- Added LPIR shape regressions for constant-index writes, dynamic-index writes, and the real fluid compute example.
- Added runtime compute coverage for dynamic indexed writes into the fluid emitter output array.

## Decisions for future reference

#### Place Paths Before Values

- **Decision:** Lower HIR places to explicit flat-lane or memory-backed access forms before materializing values.
- **Why:** Reads and writes through addressable aggregate paths should be proportional to the accessed leaf, not the whole root aggregate.
- **Rejected alternatives:** Relying on later LPIR optimization; special-casing only the fluid shader.
- **Revisit when:** Both frontends share a common frontend-neutral aggregate layout module.

#### Dynamic Memory Indexing Is In Scope

- **Decision:** Support dynamic indexing for memory-backed aggregate places in the same architecture.
- **Why:** Constant-index writes unblock the current fluid shader, but dynamic map/array writes are a natural compiler feature and use the same address model.
- **Rejected alternatives:** Leaving dynamic indexing on the select/rebuild fallback for memory-backed roots.
- **Revisit when:** Bounds policy changes from clamping to trapping or explicit diagnostics.

#### Naga Frontend Is Reference Only

- **Decision:** Borrow the architecture concept from the Naga frontend without modifying it in this plan.
- **Why:** The current compute path uses `lps-glsl`, and the Naga code is coupled to Naga handles and expression forms.
- **Rejected alternatives:** Refactoring both frontends together.
- **Revisit when:** There is a dedicated shared-frontend-layout cleanup.
