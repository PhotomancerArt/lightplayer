# 06 - Mockup Vertical Slice

## Scope of phase

Prove the factory/default object model in the mockup.

In scope:

- Add mockup tests for `registry.create_default` on static and dynamic shapes.
- Add a small default-and-mutate read helper or test-only flow that creates an
  object from shape id, inserts map entries explicitly, and mutates leaves.
- Exercise a static root, a dynamic shader param shape, a map, an option, and
  an enum discriminator path.

Out of scope:

- Removing all generated codec deserialization.
- TOML writer implementation.
- Large changes to mockup source/domain models.

## Code organization reminders

- Keep mockup tests generic; do not add hard-coded mockup-specific policy to
  `lpc-slot-codegen`.
- Prefer tests named after the generic behavior being proven.
- If a helper is generic enough to belong in `lpc-model`, move it there rather
  than hiding it in mockup tests.

## Sub-agent reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation details

Relevant files:

- `lp-core/lpc-slot-mockup/src/tests/shape_factory.rs`
- `lp-core/lpc-slot-mockup/src/tests/mod.rs`
- `lp-core/lpc-slot-mockup/src/engine/runtime.rs`
- `lp-core/lpc-slot-mockup/src/engine/shader_node.rs`

Suggested tests:

1. Static factory:
   - register mockup static shapes.
   - call `registry.create_default(ProjectDef::SHAPE_ID)`.
   - assert returned object has the project shape id and exposes default fields.

2. Static semantic default:
   - call `registry.create_default(ShaderDef::SHAPE_ID)`.
   - read `glsl_path` through slot access and assert `"main.glsl"`.

3. Dynamic factory:
   - create a `ShaderNode` dynamic param shape.
   - register it dynamically.
   - call `registry.create_default(shader_node.shape_id())`.
   - assert the returned object is slot-accessible and has a dynamic record/map
     matching the shape.

4. Map construction:
   - start from a default object with an empty map.
   - explicitly insert a default map value.
   - call `set_slot_value` on the inserted leaf.

5. Enum construction:
   - create a default object.
   - switch enum variant with default payload.
   - set a field inside that payload.

6. Non-creatable shape:
   - register or restore a shape with an explicit unsupported factory.
   - assert generic creation/deserialization reports a clear non-creatable
     shape error.

## Validate

```bash
cargo fmt -p lpc-model -p lpc-slot-codegen -p lpc-slot-mockup --check
cargo test -p lpc-slot-mockup shape_factory
cargo test -p lpc-slot-mockup mutation
```
