# Milestone 2: Core engine

## Goal

Create the first real core runtime owner: an engine that owns scheduling,
resolution, frame state, and the per-frame cache.

This milestone should prove the demand-driven execution model without trying to
port every legacy node or solve future render-product abstractions.

## Context

M1 reorganizes the codebase around an update-in-place strategy. M2 defines the
runtime owner that M1 makes room for.

The central type is likely `Engine` or `EngineRuntime`. It should own the
runtime objects that are currently separate spine pieces:

- `NodeTree`
- `Bus`
- artifact manager
- frame id / frame timing
- output provider capability
- engine-owned per-frame resolution cache

## In scope

- Define the core engine owner shape.
- Define the engine-owned resolution path:
  - nodes ask the engine/context for values;
  - nodes do not ask children directly;
  - nodes do not own the main resolver cache.
- Define query/cache keys for the first slice:
  - bus channel;
  - node output;
  - node input/consumed slot if needed;
  - texture-backed render product if needed.
- Add a same-frame cache invariant: a demanded producer runs at most once per
  frame.
- Add cycle/re-entrant demand detection for the first shape, even if diagnostics
  are minimal.
- Return versioned values using `Versioned<T>` or a closely related engine
  wrapper.
- Drive a very small demand-root flow with test/dummy nodes or a thin legacy
  adapter.

## Out of scope

- Porting all legacy nodes.
- Switching legacy source from JSON to TOML.
- Retiring `LegacyProjectRuntime`.
- Full visual/render product abstraction.
- Async resolution.
- Full cross-frame dependency tracking.

## Key decisions

- **Engine-owned resolution:** all system-level queries route through the
  engine.
- **Engine-owned per-frame cache:** `NodeEntry` should not own the main
  `ResolverCache` in the final shape.
- **Imperative node authoring:** nodes call `ctx.resolve(...)`, branch, compute,
  and publish. Do not require declarative dependency graph authoring.
- **Node-private caches are allowed:** nodes own private products like compiled
  shaders, texture buffers, selected-child state, and sampling plans.

## Suggested plan location

When ready, expand this milestone with `/plan` or `/plan-small` at:

`docs/roadmaps/2026-05-01-runtime-core/m2-core-engine/`

## Success criteria

- A new core engine owner exists.
- A demand root can trigger resolution through the engine.
- A producer demanded more than once in a frame is not recomputed.
- Resolved values carry versions.
- The next milestone can focus on source/config migration without also inventing
  the engine owner.

