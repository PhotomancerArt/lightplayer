# M1.3 Slot Shape Registration Codegen Notes

## Scope Of Work

M1.3 is pre-work before M2. It replaces hand-maintained static slot shape
registration lists with build-time generated bootstrap code.

In scope:

- Add an idempotent static shape registration path to `lpc-model`.
- Introduce a `StaticSlotShape` trait that owns shape id and shape construction.
- Keep `StaticSlotAccess` for roots with data access, but have it build on
  `StaticSlotShape`.
- Teach `#[derive(SlotRecord)]` to emit `StaticSlotShape` for root records.
- Add build-time codegen that discovers `#[slot(root)]` `SlotRecord` types and
  writes `OUT_DIR/slot_shapes.rs`.
- Generate:
  - `register_all_static_slot_shapes(...)`
  - `ensure_static_slot_shape(...)`
- Apply the generated bootstrap to `lpc-slot-mockup` and remove its manual
  static registration lists.
- Document that dynamic shape ids are per artifact or per instance when shape
  varies.

Out of scope:

- Converting real `lpc-source` defs to slot-aware fields.
- Solving final runtime shader param shape ownership.
- Replacing runtime dynamic shape registration.
- Adding committed generated files.
- Linker-section or inventory-style registration.

## User Notes And Decisions

- User wants to avoid manual registration because it is easy to forget to add or
  remove shape roots.
- User likes codegen because it is explicit and not hand-maintained.
- User is fine with `OUT_DIR` generated files if the build-dir approach works
  well.
- Static registration should support “register everything” and demand-driven
  lazy ensure.
- Dynamic shapes must not use one static Rust-type shape id when the shape
  varies per shader, artifact, or node instance.

## Current Codebase State

### Registry

- `SlotShapeRegistry` stores root shape trees by compact `SlotShapeId`.
- `register_tree(...)` currently errors on duplicate ids.
- `replace_tree(...)` exists for dynamic shape replacement.
- `SlotShape::Ref { id }` references another registered root shape.
- Snapshot/diff code expects every referenced shape id to already be present.

### Static Roots

- `StaticSlotAccess` currently owns:
  - `const SHAPE_ID`
  - `fn register_shape(...)`
- The derive macro emits `SlotAccess` and `StaticSlotAccess` for root records.
- Manual mockup registration calls:
  - `ProjectDef::register_shape`
  - `ShaderDef::register_shape`
  - `FixtureDef::register_shape`
  - `OutputDef::register_shape`
  - `TextureDef::register_shape`
  - selected engine roots

### Dynamic Roots

- `lpc-slot-mockup` manually registers the shader node dynamic shape with:
  `registry.register_tree(ShaderNode::SHAPE_ID, shader_node.shape())`.
- This is only valid in the mockup because there is one shader node. In real
  runtime code, shader param shapes may vary per artifact or per node instance,
  so static Rust type ids are wrong for dynamic record shapes.

### Codegen Conventions

- Existing shader/builtin codegen uses build scripts in other crates.
- There is no current slot-shape codegen helper.
- `lpc-slot-macros` already depends on `syn` and parses slot attributes, but
  procedural macros cannot discover all crate types.

## Key Design Direction

Use build-time codegen rather than runtime reflection or linker-section
registration.

- Build scripts are explicit at the crate boundary.
- Generated files live in `OUT_DIR`.
- Crates include generated code with:

```rust
pub mod slot_shapes {
    include!(concat!(env!("OUT_DIR"), "/slot_shapes.rs"));
}
```

Generated code should be plain Rust that calls static trait methods on
discovered root types.

## Open Questions

### Q1. Should generated files be committed?

Answer: no for M1.3. Generate into `OUT_DIR`. If this becomes too opaque, revisit
a committed generated file plus freshness check later.

### Q2. Should build codegen register nested non-root records?

Suggested answer: no. `#[slot(root)]` means registry root. Nested records remain
owned inline by their parent shape unless explicitly promoted to a root.

### Q3. How should lazy registration handle `SlotShape::Ref`?

Suggested answer: after ensuring a root shape, collect `SlotShape::Ref` ids from
that shape tree and recursively call the generated `ensure_static_slot_shape`.
If a referenced id is not known to that generated crate module and not already
registered, return a clear missing-reference error.

### Q4. Should `StaticSlotAccess::register_shape` remain?

Suggested answer: keep it temporarily as a compatibility method delegating to
`ensure_registered`. Existing code/tests use it. M2 or cleanup can rename call
sites after the generated bootstrap lands.

### Q5. Should dynamic shapes participate in build codegen?

Answer: no. Dynamic shapes are registered and replaced by the runtime owner
using artifact- or instance-specific ids.
