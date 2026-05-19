# M5 Design

## Scope

Create a standalone fluid example that uses the real authored artifact model, real project loader, real compute shader node, real fluid node, and existing fixture/output flow.

Out of scope:

- Changing `examples/basic`.
- Adding new UI features.
- Adding new shader language/runtime features.
- Deep performance tuning.

## File Structure

```text
examples/fluid/
  project.toml
  compute.toml
  compute.glsl
  fluid.toml
  fixture.toml
  output.toml

lp-core/lpc-engine/src/engine/
  project_loader.rs            # add/extend example loader tests if useful

docs/roadmaps/2026-05-12-serial-compute-shaders-fluid/m5-fluid-compute-example/
  00-notes.md
  00-design.md
  01-example-files.md
  02-loader-and-debug-validation.md
  03-local-run-and-cleanup.md
  summary.md
```

## Architecture Summary

The example should be bus-first:

```text
compute.produced.emitters
  -> [bindings.emitters] target = "bus#fluid.emitters"
  -> fluid.emitters source = "bus#fluid.emitters"

fluid.output
  -> [bindings.output] target = "bus#visual.out"
  -> fixture.input source = "bus#visual.out"

fixture.output
  -> [bindings.output] target = "bus#control.out"
  -> output.input source = "bus#control.out"
```

The compute shader is the first authored producer of structured non-visual data. It emits `lp::fluid::Emitter` values through a sentinel-array mapping. The fluid node consumes that map as slot data, runs the solver, and produces a visual product. The fixture samples that visual product into control data, and the output writes the control product.

## Main Components

- `compute.toml`: declares compute shader produced emitter map.
- `compute.glsl`: serial tick shader that writes one or more emitters.
- `fluid.toml`: declares solver config and emitter/visual bindings.
- `fixture.toml`: copied/adapted from `examples/basic`, using direct sampling.
- `output.toml`: copied/adapted from `examples/basic`.
- Loader/debug validation: proves `examples/fluid` loads and produced data is visible through existing project read/debug mechanisms.
