# Slot Domain Cutover Roadmap

## Motivation And Rationale

LightPlayer now has the core pieces of a real slot-domain model, but the production path is still split across old and new ideas:

- source defs loaded from TOML,
- runtime nodes with legacy produced/state access,
- legacy `ProjectResponse` detail projection,
- generic slot sync and mutation types,
- a slot mirror in `lpc-view`,
- node-specific debug UI panels.

The goal of this roadmap is to make slots the normal domain boundary across source, engine, wire, view, and debug UI. A node's authored definition, runtime state, dynamic params, and produced outputs should be exposed as structured, versioned slot roots instead of bespoke per-node wire shapes.

This is a significant cutover. The work should allow temporary bridge code, but the roadmap ends with a cleanup milestone so the bridge does not become permanent architecture.

## Architecture And Design

```text
project.toml / node.toml
        |
        v
Source NodeDef as StaticSlotAccess
        |
        v
Engine node roots: source / state / params / output
        |
        v
Project sync: slot registry + watched root snapshots/patches
        |
        v
lpc-view SlotMirrorView
        |
        v
generic debug egui + opt-in resource payload previews
```

The source side should start the cutover because real node definitions already exist and can be exposed as `StaticSlotAccess` without changing tick semantics. Runtime exposure follows once the source and wire/view bridge are proven.

The project sync path should carry slot data alongside existing legacy project responses at first. This gives the client and UI something real to consume while legacy `NodeState` detail projection remains available as a safety net. The later cleanup milestone removes the old detail path once parity is demonstrated.

Watching should move from "node detail" to "slot roots." The first production version can keep a simple convention:

- `source` for authored node definition data.
- `state` for runtime state and introspection.
- `params` for dynamic shader/authored runtime params where needed.
- `output` for produced node outputs.

The debug UI may keep an "all detail" style control, but it should mean "watch conventional state roots" rather than "request node-specific legacy detail objects."

Resources should stay lightweight by default. Resource refs and metadata should sync as normal slot/wire data, but raw texture/buffer bytes should be requested explicitly by UI interest. This preserves the low-bandwidth path needed for real devices.

## Alternatives Considered

### One Big Replacement

Replace `ProjectResponse`, legacy `NodeState`, node-specific UI, and runtime produced access in one patch.

Rejected because the engine, resolver, resource projection, and UI are too entangled. A bridge is less pure but much easier to validate.

### Keep Detail Tracking As The Main Concept

Keep `WireNodeSpecifier` / node detail as the public model and feed slot data through it.

Rejected because it preserves the wrong abstraction. The real model is watching slot roots and resource payload interest, not requesting opaque node details.

### Start With Runtime State

Expose engine state/params/outputs first.

Rejected as the first slice because it immediately touches resolver semantics and runtime product/resource ownership. Source defs are a cleaner first proof on production data.

### Build Full Mutation Now

Add client-driven mutation as part of the cutover.

Deferred. Mutation is important, but the engine needs cleanup and stronger mutation boundaries after the slot-domain cutover.

## Risks

- Runtime slot exposure crosses resolver, resources, produced outputs, and node lifecycle.
- Generic UI may reveal metadata gaps in `SlotShape` / semantic leaf hints.
- The bridge period can create duplicate logic unless the cleanup milestone is treated as required.
- Resource payloads can accidentally become too eager and too large for device use.
- Existing examples and integration tests may break during early phases.
- Shader params are dynamic and will pressure shape updates, registry versions, and client pruning.

## Scope Estimate

This roadmap should be implemented through full plans for most milestones. The effort spans several crates and should be expected to produce multiple reviewable commits.

