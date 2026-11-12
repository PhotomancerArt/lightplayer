# M5 Notes

## Scope

Build a new `examples/fluid` project that demonstrates the intended compute-fluid dataflow:

```text
compute shader -> fluid node -> fixture -> output
```

This milestone should keep `examples/basic` unchanged as the small steady-state baseline.

## Current State

- `NodeDef::ComputeShader` is loaded from `kind = "shader/compute"`.
- Compute shader TOML defines dynamic `consumed` and `produced` slot maps.
- Compute shaders can produce `MapSlot<u32, FluidEmitter>` through sentinel-array mapping.
- `FluidDef.emitters` is a consumed map slot with `merge = "by_key"` and references `lp::fluid::Emitter`.
- `FluidNode` can consume bound emitter maps, step the Q32 solver, and produce a visual product.
- Existing `examples/basic` already has the desired fixture/output ring geometry and output binding shape.

## Example Shape

Proposed new tree:

```text
examples/fluid/
  project.toml
  compute.toml
  compute.glsl
  fluid.toml
  fixture.toml
  output.toml
```

`texture.toml` is intentionally omitted. The fluid node produces the visual product directly.

## Open Questions

### Q1. Should this be `examples/fluid` or `examples/fluid-basic`?

Suggested answer: `examples/fluid`. It is short, clear, and likely to become the canonical fluid example.

Answer: `examples/fluid`.

### Q2. Should we clone the `examples/basic` fixture exactly?

Suggested answer: yes for M5. Reusing the ring fixture keeps the scope focused on the new compute-fluid path.

### Q3. How fancy should the compute shader be?

Suggested answer: modest. Generate a few moving emitters with deterministic math and avoid requiring new shader language features. The example should show the architecture working before becoming an art piece.

### Q4. Should M5 add profile reports?

Suggested answer: run local/dev validation in the plan; capture ESP32 profile as follow-up if the example boots cleanly. Profiling is useful, but first we need a stable source example.

## User Notes

- Use a new `examples/fluid`; do not replace `examples/basic`.
- This is meant to validate the domain model, not become final UI polish.
- The example should exercise bus-first binding, not direct node-to-node coupling.
