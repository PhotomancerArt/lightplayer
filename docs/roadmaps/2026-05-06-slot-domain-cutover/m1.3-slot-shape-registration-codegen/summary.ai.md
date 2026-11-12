# M1.3 Slot Shape Registration Codegen Summary

## What Was Built

- Added `StaticSlotShape` in `lpc-model` as the shape-only static root trait.
- Added idempotent `SlotShapeRegistry::ensure_tree`, `contains`, and nested
  `SlotShape::Ref` collection.
- Updated `#[derive(SlotRecord)]` root output to emit `StaticSlotShape` plus
  the existing `StaticSlotAccess` compatibility impl.
- Added `lpc-slot-codegen`, a host-only build helper that discovers
  `#[derive(SlotRecord)] #[slot(root)]` types and writes `OUT_DIR/slot_shapes.rs`.
- Switched `lpc-slot-mockup` static shape registration to the generated
  bootstrap while keeping `ShaderNode` dynamic shape registration explicit.
- Added tests for static registration coverage and idempotent generated ensure.

## Decisions For Future Reference

#### Static Shape Trait Split

- **Decision:** `StaticSlotShape` owns shape id and shape construction;
  `StaticSlotAccess` layers data access on top.
- **Why:** Some generated/bootstrap code only needs static shape roots, while
  runtime walking still needs values.
- **Rejected alternatives:** Keep all static shape behavior on
  `StaticSlotAccess`.
- **Revisit when:** The derive macro gains richer static root categories.

#### Idempotent Ensure

- **Decision:** Static registration uses `ensure_tree`; strict
  `register_tree` remains duplicate-erroring.
- **Why:** Generated bootstrap may be called from multiple paths and should be
  safe when the shape is identical, while dynamic registration still needs
  explicit lifecycle semantics.
- **Rejected alternatives:** Make `register_tree` idempotent globally.
- **Revisit when:** Dynamic shape replacement gains a stricter owner model.

#### OUT_DIR Codegen

- **Decision:** Generated registration lives in `OUT_DIR`, not committed source.
- **Why:** It removes hand-maintained lists without adding generated-file
  freshness churn to the repo.
- **Rejected alternatives:** Committed generated Rust, linker-section
  registration, runtime inventory.
- **Revisit when:** Debuggability of generated bootstrap becomes a real pain.

#### Dynamic Shape Boundary

- **Decision:** Build codegen only registers static Rust-authored roots.
  `ShaderNode` keeps manual dynamic shape registration.
- **Why:** Shader params vary by shader artifact/node instance, so one Rust type
  id would be semantically wrong.
- **Rejected alternatives:** Include every `SlotAccess` root in generated static
  registration.
- **Revisit when:** The engine has artifact-/instance-owned dynamic shape ids.
