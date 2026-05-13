# General Slot Mutation Notes

## Scope of work

- Replace the current narrow, clock-specific project slot mutation path with a general mutation system.
- Make mutation editable by default for authored slot data, with explicit opt-out for exceptional cases.
- Align slot-shape generation, client UI behavior, and server mutation handling so the default authored-node experience is editable without per-node special cases.
- Avoid breaking the existing slot-domain architecture or introducing host-only shortcuts.

## Current state of the codebase relevant to the task

- Slot editability is currently driven by `SlotPolicy` on record fields.
  - `lp-core/lpc-model/src/slot/slot_policy.rs`
  - `SlotPolicy::default()` is `read_only_persisted()`.
- `#[derive(SlotRecord)]` currently emits field shapes with semantics only, not policy overrides.
  - `lp-core/lpc-slot-macros/src/record.rs`
  - Generated fields use `slot::shape::field_with_semantics(...)`, so ordinary authored fields inherit read-only policy.
- The debug UI obeys that policy directly.
  - `lp-cli/src/debug_ui/slot_render.rs`
  - Editors only render when `policy.writable` and the leaf editor supports the value type.
- Clock controls are a manual exception.
  - `lp-core/lpc-model/src/nodes/clock/clock_controls.rs`
  - Those fields explicitly use `field_with_policy(..., SlotPolicy::writable_transient())`.
- The server mutation path is also a narrow exception.
  - `lp-core/lpc-engine/src/engine/slot_mutation.rs`
  - It only accepts `node.<id>.def` roots, and then only mutates `NodeDef::Clock` leaves via hard-coded path matching.
- Existing planning docs already describe a more general target than the current implementation.
  - `docs/plans/2026-05-12-clock-node-time-mutation/04-project-slot-mutation.md`
  - That phase says `SetValue` should apply to value leaves on `node.<id>.def` roots, but the implementation stopped at clock-specific typed mutation.
- Slot shape metadata already has the pieces needed for a general policy-aware mutation system.
  - `lp-core/lpc-model/src/slot/slot_shape.rs`
  - `SlotFieldShape` carries `policy` and `semantics`.
- Client-side optimistic validation is already mostly generic.
  - `lp-core/lpc-view/src/slot/apply.rs`
  - `validate_value_at(...)` and `data_version_at(...)` resolve arbitrary paths through shape/data trees.

## Open questions

### 1. What does "everywhere by default" cover in this plan?

Context:

- Today the real mutation entrypoint only targets `node.<id>.def`.
- Runtime state roots (`node.<id>.state`) are also synced into the debug UI.
- Making all synced roots mutable by default could unintentionally expose ephemeral runtime internals as editable API.

Suggested answer:

- Treat "everywhere by default" as "every authored node-def value leaf by default" for this plan.
- Keep runtime state roots opt-in and explicitly out of scope unless a later use case appears.
- Still design the policy and mutation plumbing so runtime-state opt-in is possible later without another special-case rewrite.

Resolution:

- Confirmed with the user.
- This plan will hardcode the mutable-root scope to authored `node.<id>.def` roots for now.
- `node.<id>.state` roots remain out of scope and non-mutable by default in this phase.

### 2. Where should the writable-by-default rule live?

Context:

- The current default is global (`SlotPolicy::default()`), but changing that globally would also affect places that are not authored defs.
- The derive macro is a natural place to encode authored-record defaults, but some records represent runtime state or helper structures.

Suggested answer:

- Do not change `SlotPolicy::default()` globally.
- Add an explicit authored-record/container-level default in `SlotRecord` derive or an adjacent slot-shape builder path, so authored defs can default writable without silently changing unrelated record shapes.

### 3. How should opt-out be expressed?

Context:

- There is no current field-level `#[slot(...)]` policy attribute for read-only or writable overrides in the derive.
- Clock controls currently bypass the derive with a handwritten record shape.

Suggested answer:

- Add explicit field/container policy attributes in the derive macro so authored defs can default writable while exceptional fields opt out with a readable source-level annotation.
- Preserve the ability for handwritten shape implementations to keep using `field_with_policy(...)`.

## Notes from the user that should influence the plan

- Mutation support should be general, not clock-specific.
- Mutability should be the default, with opt-out available but expected to be rare.
- The current clock-specific mutation code is seen as a design smell and should be removed in favor of a generic path.
