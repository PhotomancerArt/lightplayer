# Milestone 4: Legacy node runtime port

## Goal

Port the legacy MVP node behavior onto the core engine contracts while keeping
the old engine available as a comparison path.

The target behavior is the existing shader -> fixture -> output flow expressed
through engine-owned resolution and demand roots.

## Context

M2 creates the core engine owner/scheduler. M3 migrates legacy-authored source
toward TOML and `lpc-source`. M4 ports actual runtime behavior onto that new
path.

This milestone should preserve behavior first. It is not the moment to optimize
fixture sampling or invent the final render-product family.

## In scope

- Port or adapt the MVP legacy runtime nodes:
  - shader visual/producer;
  - fixture demand root;
  - output flush target;
  - texture compatibility if required by shader/fixture behavior.
- Make fixtures demand roots in the core engine.
- Route child/output/bus reads through engine-owned resolution.
- Use the engine per-frame cache so demanded producers run at most once per
  frame.
- Use `Versioned<T>` versions for node-private cache decisions where helpful.
- Preserve existing shader compile/execute behavior, including embedded JIT
  requirements.
- Add parity tests against current legacy behavior where practical.

## Out of scope

- Retiring `LegacyProjectRuntime`.
- Removing all legacy APIs.
- Replacing texture-backed rendering with sampled render products.
- Async/parallel scheduler execution.
- Full visual model beyond the legacy MVP slice.

## Key decisions

- **Behavior parity before optimization:** keep the old flow working in the new
  engine before changing its render model.
- **Texture-backed is acceptable here:** render products can be named in the
  contract, but the concrete first product may still be a texture.
- **Demand-root fixture flow:** fixtures drive the frame; outputs flush after
  fixture-side mutation.

## Suggested plan location

When ready, expand this milestone with `/plan` or `/plan-small` at:

`docs/roadmaps/2026-05-01-runtime-core/m4-legacy-node-runtime-port/`

## Success criteria

- The core engine can run the legacy MVP flow.
- Shader/fixture/output behavior matches the old runtime closely enough for
  comparison tests.
- Producer work is demand-driven and same-frame cached.
- The old engine remains available until M5 cutover.

