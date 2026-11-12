#### Slots Are The Domain Boundary

- **Decision:** Source defs, runtime state, params, and outputs should converge on slot roots.
- **Why:** Slots are the unit of versioned structured data, generic sync, and generic UI rendering.
- **Rejected alternatives:** Continue adding node-specific state/config wire types; keep slots as mockup-only infrastructure.
- **Revisit when:** A concrete node domain cannot fit the slot model without contortion.

#### Start With Source Defs

- **Decision:** The first production cutover slice should expose real source node defs as slot roots.
- **Why:** Source defs are real domain data but do not require changing tick/resolver semantics first.
- **Rejected alternatives:** Start with runtime state; replace project sync in one pass.

#### Watch Slot Roots, Not Node Details

- **Decision:** Replace node detail tracking with explicit slot-root watch interest.
- **Why:** The client wants structured data roots, not opaque per-node detail objects.
- **Rejected alternatives:** Keep `WireNodeSpecifier` as the main model; add slots inside legacy node detail.

#### Keep Resource Payloads Opt-In

- **Decision:** Sync resource metadata/skeletons separately from raw payload bytes.
- **Why:** Real devices need low-bandwidth sync, and UI previews should request bytes only when needed.
- **Rejected alternatives:** Always include texture/buffer bytes with state; hide resources behind node-specific detail.

#### Hard Legacy Removal After Reference Tag

- **Decision:** After tag `2026-05-07-pre-legacy-remove`, delete the legacy project sync/detail/UI path and rebuild canonical project sync around slots.
- **Why:** There are no external users, the old debug UI and messages are being rebuilt, and keeping legacy alive would force new slot-domain work through the wrong conceptual shape.
- **Rejected alternatives:** Promote `LegacyProjectResponse` into the new protocol; keep bridge fields working until parity; maintain permanent dual sync models.
- **Revisit when:** A deleted legacy path turns out to contain behavior that is not recoverable from the reference tag/worktree.

#### Mutation Is Future Work

- **Decision:** Do not make client-driven source/artifact mutation part of the main cutover.
- **Why:** The engine needs cleanup and stronger mutation boundaries after the slot-domain path is real.
- **Rejected alternatives:** Build full editing into this roadmap.
- **Revisit when:** Legacy detail is removed and the engine API is ready for a real UI.

#### Runtime Slot Identity Uses SlotPath

- **Decision:** Produced and consumed runtime slot identity should move to `SlotPath`.
- **Why:** `ValuePath` is for nested leaf values, not slot identity.
- **Rejected alternatives:** Continue treating path-through-value and slot identity as the same concept.

#### Generic UI With Minimal Metadata

- **Decision:** Add only the metadata needed for debug generic rendering during this cutover.
- **Why:** The UI must prove the model without prematurely designing the final product UI vocabulary.
- **Rejected alternatives:** Keep fully node-specific panels; design a full semantic editor system now.
