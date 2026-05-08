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

This is a significant cutover. Earlier milestones prepared a bridge, but the active migration strategy changed after tag `2026-05-07-pre-legacy-remove`: the old project sync, legacy detail model, and old debug UI are reference material, not compatibility obligations. From M2.2 onward the roadmap intentionally deletes the legacy path and rebuilds the canonical stack around slots.

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

The source side starts the cutover because real node definitions already exist and can be exposed as `StaticSlotAccess` without changing tick semantics. Runtime exposure follows once canonical source sync and the generic view path are real.

The project sync path should be rebuilt as slot-first canonical messages. The old `LegacyProjectResponse` / `LegacyNodeState` detail path was useful as scaffolding, but keeping it alive now adds compatibility gravity without serving external users. The reference tag and worktree preserve it for comparison.

Watching should move from "node detail" to "slot roots." The first production version can keep a simple convention:

- `source` for authored node definition data.
- `state` for runtime state and introspection.
- `params` for dynamic shader/authored runtime params where needed.
- `output` for produced node outputs.

The rebuilt debug UI may keep an "all detail" style control, but it should mean "watch conventional state roots" rather than "request node-specific legacy detail objects."

Resources should stay lightweight by default. Resource refs and metadata should sync as normal slot/wire data, but raw texture/buffer bytes should be requested explicitly by UI interest. This preserves the low-bandwidth path needed for real devices.

## Alternatives Considered

### One Big Replacement

Replace `ProjectResponse`, legacy `NodeState`, node-specific UI, and runtime produced access in one patch.

Initially rejected because the engine, resolver, resource projection, and UI were too entangled. Revisited after M2.1: because there are no external users and the old UI/messages are being rebuilt anyway, this roadmap now takes a staged demolition-and-rebuild path instead of preserving a compatibility bridge.

### Bridge Then Delete

Add slot sync alongside current project sync first, then remove legacy detail projection after parity.

Superseded after tag `2026-05-07-pre-legacy-remove`. The bridge was sensible while the slot model was still uncertain. Once source defs, value leaves, shape bootstrap, and the mockup were proven, carrying both protocols became more confusing than useful.

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
- Deleting the old sync/UI path will cause intentional compile breakage while the canonical path is rebuilt.
- Resource payloads can accidentally become too eager and too large for device use.
- Existing examples and integration tests may break during early phases.
- Shader params are dynamic and will pressure shape updates, registry versions, and client pruning.

## Scope Estimate

This roadmap should be implemented through full plans for most milestones. The effort spans several crates and should be expected to produce multiple reviewable commits.
