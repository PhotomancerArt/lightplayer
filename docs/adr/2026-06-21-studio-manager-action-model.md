# ADR: Studio Manager Action Model

Date: 2026-06-21

## Status

Superseded by [2026-06-21 Studio UX Layer](./2026-06-21-studio-ux-layer.md)

## Context

Studio has outgrown a single provisioning-flow model. The UI needs a device/link
surface now, but the same product model also needs to support future server,
project, CLI, and agent consumers. A flat `StudioActionKind` made every action
look like an app-global operation and did not clearly say which manager owned
the state or decision.

At the same time, the current web UI still renders one device-oriented journey.
The architecture needs to move toward link/server/project managers without
forcing a full presentation rewrite in the same step.

## Decision

Studio core exposes three manager-owned action vocabularies:

- `LinkActionRequest` for provider choice, endpoint access, link open/close,
  target probing, flashing, reset, diagnostics, and link issues.
- `ServerActionRequest` for `lp-server` protocol status and server project-state
  reads.
- `ProjectActionRequest` for project attachment, project loading/deployment,
  inventory reads, and project-local navigation.

`StudioActionKind` is now an app-wide wrapper around those manager-local
requests. Generic consumers dispatch `StudioActionKind`, while manager code owns
its local request vocabulary.

Each manager read model exposes `available_actions()`, returning
`AvailableAction<T>`. `AvailableAction` combines:

- a dispatchable action payload;
- an `ActionDescriptor` for label, summary, category, and history policy;
- enablement;
- presentation priority;
- optional confirmation metadata for risky actions.

`StudioState::available_actions()` combines link, server, and project actions
into app-wide `StudioActionKind` values for UI and agent consumers.

The visible link vocabulary no longer uses `ProviderSelected` or
`EndpointGranted` as flow states. Provider selection is catalog state, and
endpoint discovery proceeds directly into `LinkState::Opening` with a connect
effect. This keeps the happy path friction-free and leaves user-visible states
for moments where the user or system has meaningful work.

## Consequences

- UI and future agents can ask core for the current state and valid actions
  without hard-coding flow-specific buttons.
- Action descriptors remain shared program documentation rather than duplicated
  component copy.
- The runtime boundary stays effect/event based; manager actions do not perform
  I/O directly.
- The current web UI can keep rendering transitional server/project milestones
  through `LinkState` until the presentation is split into link/server/project
  panes.
- Endpoint discovery now auto-continues into link opening instead of exposing a
  separate endpoint-granted stop.

## Alternatives Considered

- Keep a flat `StudioActionKind`: simpler in the short term, but it obscures
  ownership and makes agent/tool documentation harder to organize.
- Move all server/project states out of `LinkState` immediately: cleaner final
  model, but it would couple this core action pass to a larger web UI rewrite.
- Let UI components derive available actions directly from state: fast for the
  current UI, but it would duplicate policy and make non-UI consumers less
  honest.

## Follow-ups

- Split the web presentation around link/server/project read models.
- Move remaining transitional server/project journey states out of `LinkState`
  once the UI consumes the new manager states directly.
- Use `AvailableAction` for visible controls instead of component-local button
  decisions.

## Update 2026-06-22

The manager action idea became the active `lpa-studio-ux` model: `LinkUx`,
`ServerUx`, and `ProjectUx` own typed operations and expose contextual
`UxAction` values. `AvailableAction` and the effect/event runtime boundary were
removed with the old core/runtime crates.
