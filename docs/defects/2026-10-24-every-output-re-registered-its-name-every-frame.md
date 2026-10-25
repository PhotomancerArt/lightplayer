---
status: fixed
found: 2026-10-24      # how: the frame-plan checker (PlanSlot check mode) on the corpus differential, small-dome
fixed: 2026-10-24
area: lp-core/lpc-engine engine output identities (EngineResolveHost::register_output_identity) × the fixture's patch-resolution cache
class: assumed-context   # "not Alive" was read as "dead"; the caller itself is Executing, not dead
related:
  - lp2025/2026-10-24-0208-engine-frame-plan (P3)
  - docs/adr/2026-10-24-frame-plan-once-per-edit.md
  - lp-core/lpc-engine/tests/frame_plan_differential.rs
---
# Every output re-registered its name every frame

**Symptom** — Nothing on a wall: the bytes were right. The cost was hidden.
Every output's `consume` moved `output_identity_revision`, every frame, so
every patched fixture's patch-resolution cache (keyed on that revision)
missed and re-resolved its patch document every frame. Found when the
output's frame plan, keyed on the same revision, rebuilt on every steady
frame and the differential's "a steady frame builds no plan" check fired.

**Cause** — Two faults, one masking the other:

1. `register_output_identity` prunes outputs that are no longer alive before
   recording the caller's name, and "alive" meant `NodeEntryState::Alive`.
   The caller is mid-`consume`: its runtime is out of the tree, `Executing`.
   So each output swept *its own* entry as dead, re-inserted it, and counted
   that as a change.
2. The revision was stamped with the frame's revision, not counted. Two
   outputs registering in one frame left one stamp, so a cache keyed between
   them could not see the second. The first fault hid this: the revision
   moved every frame anyway. With it fixed, small-dome's second output
   (`Box 2`) kept a plan built before its first producer's patch had seen
   that name — the plan checker named the field.

**Fix** — The caller is exempt from its own sweep, and the sweep runs once
per structure epoch (a node only stops being alive through a structural
edit; only the loader marks one `Failed`). The revision is a change counter.
A producer's patch can still re-resolve on a frame nothing structural moved,
so the output's plan also keys on its producers' `control_patch_generation`.

**Regression test** — `tests/frame_plan_differential.rs` (every corpus
project, strict-checked mode, and zero structural work on every steady
frame); small-dome is the project that exercises the second fault.
