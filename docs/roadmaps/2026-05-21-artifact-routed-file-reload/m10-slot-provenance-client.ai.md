# Milestone 10: Slot Resolution Probes

## Title And Goal

Implement **on-demand provenance** via **project-read probes** — not on the
normal value read path. When the client wants to know how a consumed slot
resolved, it attaches an `ExplainSlot` probe to a `project_read` request (or
re-derives locally on the host when it already holds bindings + ChangeSet).

Runtime ticks and ordinary slot reads stay lean: **value only**, no provenance
structs on `Production` or registry hot paths.

## Prerequisites

- **M6** engine cutover — consumed-slot resolution reads effective defs from
  `NodeDefRegistry` (bindings → registry def).
- **ChangeSet roadmap** green — overlay + commit in mainline registry.
- Resolver trace events shaped for explain output (see M3.5 resolver notes).

## Existing hooks

Wire types already exist; engine returns `Unsupported` today:

- `lpc-wire`: `ExplainSlotProbeRequest { node, slot, include_trace }`,
  `ExplainSlotProbeResult`, `SlotExplanation { value, trace }`
- `ProjectProbeRequest::ExplainSlot` — piggybacks on `project_read` beside
  normal queries (same pattern as `RenderProduct` probe)
- `Engine::read_project_explain_slot_probe` — stub in `project_read_probes.rs`

## Suggested Plan Location

`docs/roadmaps/2026-05-21-artifact-routed-file-reload/m10-slot-provenance-client/`

## Scope

In scope:

- **Implement `ExplainSlot` probe** in engine: run resolver for `(node, slot)`,
  collect effective value + optional trace (`include_trace`).
- Trace steps cover the resolution cascade:
  **binding(s) / merge** → **effective registry def** (overlay ∪ base) →
  produced-slot fallback when applicable.
- Host **`lpa-client` helper** to attach explain probes to project reads and
  parse `SlotExplanation` for UI badges.
- **Optional local re-derive** in host client when it holds bindings +
  ChangeSet + overlay membership — same cascade, no round-trip (inspector /
  offline editing).

Out of scope:

- Provenance on every consumed-slot resolve or tick read (ESP32 memory).
- Dedicated server-side provenance logic in `lpa-server` beyond forwarding
  `project_read` probes to engine.
- Registry `explain_slot()` API — defer unless probe + local re-derive prove
  insufficient.
- Full inspector UI — hooks only.

## Key Decisions

- **Probe, not pollute reads:** Normal reads deliver effective values; explain
  is a separate request-scoped probe.
- **Client chooses when:** UI adds `ExplainSlot` probe only for inspected slots
  (or re-derives locally on host).
- **No lpa-server v1 logic:** Server forwards probes; engine executes explain.
- **May revisit:** Thin/remote clients without local ChangeSet may need richer
  wire explain or registry helpers later.

## Deliverables

- `read_project_explain_slot_probe` implemented (resolver + trace).
- `lpa-client` probe attachment + optional local `SlotProvenance` helper.
- Tests: binding wins, overlay wins over committed def, merge contributors in
  trace when `include_trace`.

## Dependencies

- M6 engine cutover.
- ChangeSet roadmap complete (overlay + effective reads in registry).

## Execution Strategy

Small plan after M6/M7 stabilize. Primary work is engine probe implementation +
client wiring; no ChangeSet roadmap changes.

Suggested chat opener:

> M10: implement ExplainSlot probe (wire exists, engine stub) — bindings then
> effective def then trace. Agree?
