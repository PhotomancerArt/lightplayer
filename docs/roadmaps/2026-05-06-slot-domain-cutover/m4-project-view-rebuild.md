# Milestone 4: Project View Rebuild

## Title And Goal

Rebuild `lpc-view` and client-side project state around canonical sync and `SlotMirrorView`.

## Suggested Plan Location

`docs/roadmaps/2026-05-06-slot-domain-cutover/m4-project-view-rebuild/`

## Scope

In scope:

- Replace legacy `ProjectView` detail/config state with canonical node index, slot mirror, watch state, and resource cache.
- Apply canonical full syncs and incremental patches.
- Keep resource summaries/payloads separate from generic slot data.
- Provide client convenience APIs for watching slot roots and requesting resource payloads.
- Remove typed `Box<dyn NodeDef>` client config mirrors from the active view path unless intentionally reintroduced as helpers over slots.

Out of scope:

- Generic egui rendering.
- Runtime node state/params/output exposure beyond what M3 provides.
- Client-driven mutation beyond existing slot mutation mirror primitives.

## Key Decisions

- `SlotMirrorView` is the authoritative generic data mirror on the client.
- The client view should know node identity/status and resource cache state, but not depend on node-specific legacy state shapes.
- Convenience accessors are allowed if they sit on top of generic canonical sync data.

## Deliverables

- `lpc-view` consumes canonical project sync responses.
- `lpa-client` request/response conversion uses canonical message types.
- Tests prove client full sync, incremental patches, node pruning, and resource cache updates.

## Dependencies

- Milestone 3 canonical project sync rebuild.

## Execution Strategy

Full plan. Rebuild the client mirror before rebuilding UI so the UI has one coherent API to consume.
