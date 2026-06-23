# ADR 2026-06-23: Studio Hierarchical UX Node Dispatch

## Status

Accepted.

## Context

The Studio device manager started with a small static UX tree:

```text
StudioUx
  DeviceUx
  ProjectUx
```

Exact target matching was enough for that shape. The project editor needs more
dynamic action targets: node tree, individual nodes, slot rows, assets, changes,
bus views, probes, and eventually binding controls attached to slot rows.

Several designs were considered:

- Keep exact static dispatch and add ad hoc string checks as needed.
- Add a non-owning `UxRegistry` or router that maps target prefixes to owners.
- Add a registry-owned component tree of boxed dynamic UX nodes.
- Make `UxNodeId` path-shaped and dispatch hierarchically through the explicit
  ownership tree.

## Decision

Studio will use path-shaped `UxNodeId` values and owner-local hierarchical
dispatch.

The ownership tree remains explicit Rust structs:

```text
StudioUx
  owns DeviceUx
  owns ProjectUx
```

The address tree is a UX path:

```text
studio.device
studio.project
studio.project.node_tree
studio.project.node.<id>
studio.project.node.<id>.slot.<slot-path>
studio.project.asset.<id>
studio.project.changes
studio.project.bus
```

`StudioUx` owns top-level routing. It handles `studio.device` and routes
`studio.project` plus `studio.project.*` to project ownership. `ProjectUx` owns
interpretation of project-local subtargets.

Actions remain in-process typed values. The target address identifies the UX
surface; the boxed typed operation identifies what to do. This decision does
not introduce serialized action commands or a remote action protocol.

## Consequences

The model keeps Rust ownership simple. There is no weak-reference graph,
`Rc<RefCell<_>>` ownership tree, or boxed async node runtime.

Dynamic addresses do not imply dynamic object ownership. A slot row can be
addressed without being stored as an independently owned UX node.

Dispatch has some manual machinery. Each owner must parse and handle its own
subtree clearly, with explicit errors for unknown targets or wrong operation
types.

A future `UxRegistry` is still possible if Studio grows non-tree routing,
plugin-style mounting, or cross-cutting introspection needs. If introduced
later, it should build on the path-shaped `UxNodeId` model rather than replacing
typed in-process actions.
