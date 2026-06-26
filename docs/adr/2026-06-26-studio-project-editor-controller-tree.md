# ADR: Studio Project Editor Controller Tree

## Status

Accepted

## Context

The Studio project editor is moving from a mock-ish project card surface toward
the real node editor UI. The synced LightPlayer client model already has a
`ProjectView` mirror with a node tree, slot mirror, shape registry, resources,
and runtime summary. That mirror is protocol/client state: it should not own
Studio interaction state, product subscription intent, slot expansion, or future
edit/binding behavior.

The node UI is also intentionally data-driven. `UiNodeView` and its child
`Ui*` structs let stories, tests, and web components render the same concepts
without requiring a live project connection.

Studio therefore needs a middle layer between `ProjectView` and `UiNodeView`:
a UI-framework agnostic controller tree that reconciles against mirror changes,
preserves local state for stable model addresses, and emits DTOs later.

The existing `ControllerId` format used dotted paths such as
`studio.project.node.4.slot.palette.primary`. That collided with existing model
path grammars: `TreePath` uses `/` and `.`, while `SlotPath` uses `.` and
bracketed map keys. Real slot paths such as `params["phase.offset"].label`
made dotted controller ids ambiguous.

## Decision

Studio project editing keeps four trees with distinct roles:

- **Mirror tree:** `lpc_view::ProjectView`, updated from LightPlayer sync
  responses. It has no Studio UI concepts.
- **Controller tree:** Studio project/node/slot controllers in
  `lpa-studio-core`. This is the UI-independent business logic layer. It owns
  reconciliation, action addressability, local interaction state, and future
  product subscription/edit/binding intent.
- **DTO tree:** render data such as `UiNodeView` and `UiConfigSlot`. It is pure
  data for stories and renderers.
- **Component tree:** Dioxus/web components and browser-local view state such
  as popovers, animations, and transient layout state.

Project controller internals are organized by domain object:

```text
app/project/node/*
app/project/slot/*
```

Project nodes use a split identity model:

- `ProjectNodeAddress` wraps the stable authored `TreePath` and is the
  controller key.
- `ProjectNodeTarget` carries both the stable address and current runtime
  `NodeId` for action targets.

Project slots are addressed by:

```text
ProjectSlotAddress {
    node: ProjectNodeAddress,
    root: ProjectSlotRoot,
    path: SlotPath,
}
```

Slot controllers are recursive and exist for container and leaf slots. This
gives expansion state, future dirty state, validation, binding, and edit actions
an addressable home.

`ControllerId` now uses `|` as its segment separator instead of `.`. Project
targets can therefore carry canonical model paths as readable payload segments:

```text
studio|project|node|nid|3|path|/demo.project/orbit.shader
studio|project|node|nid|3|path|/demo.project/orbit.shader|slot|def|path|config.brightness
```

Only payloads containing `|` or `%` need escaping.

## Consequences

The project editor can preserve local node and slot state across mirror updates
by matching stable model addresses instead of current runtime ids.

Action targets still carry `NodeId`, so future server operations do not need to
rediscover runtime handles from path alone.

The controller id grammar no longer conflicts with `TreePath`, `SlotPath`, or
binding/product endpoint conventions. Existing Studio dispatch remains
hierarchical, but ids are now visibly controller-local rather than pretending to
be model paths.

The current pre-M3 project workspace can continue using legacy node id targets
while new typed targets are introduced. That compatibility is temporary; later
milestones will project real `ProjectView` data through the controller tree and
remove old project node/slot DTOs.

The controller tree does not render directly. Rendering still goes through DTOs
so the component library and story fixtures remain data-driven.

## Alternatives Considered

- Keep dotted `ControllerId` paths and percent-encode model paths inside them.
  Rejected because it makes common node and slot paths unreadable and keeps the
  controller grammar in conflict with model grammars.
- Key node controllers by `NodeId`.
  Rejected because `NodeId` is runtime identity. It is useful for actions but
  not stable enough to preserve local Studio state across reconnects/reloads.
- Put all new types under an `app/project/controller_tree/` module.
  Rejected because it names the mechanism instead of the domain vocabulary.
  Node and slot concepts deserve their own module map.
- Render directly from `ProjectView`.
  Rejected because `ProjectView` is a protocol mirror, not an owner of Studio
  interaction state or web-independent UI policy.
