# ADR: Studio UX Layer

- **Status:** Accepted
- **Date:** 2026-06-21
- **Deciders:** Photomancer
- **Supersedes:** [2026-06-18 Studio Action And Session Architecture](./2026-06-18-studio-action-session-architecture.md)

## Context

The first Studio prototype split UI-independent state from runtime execution:

```text
StudioAction -> StudioState + StudioEffect -> StudioEvent -> StudioState
```

That proved the browser-worker simulator and hardware provisioning paths, but it
made ownership hard to reason about. Effects such as opening a link, creating a
client, loading a project, and reading inventory were not merely UI effects;
they were the actual application logic. The web app also still composed browser
runtime routing, which meant the split did not fully hide implementation
mechanics from presentation code.

Studio needs a middle layer that is UI-independent and product-shaped, but that
actually owns the services below it. The same layer should be useful to web UI,
future CLI/desktop shells, tests, and agents.

## Decision

Add `lpa-studio-ux` as the Studio UX layer:

```text
lpa-link / lpa-client / protocol services
        owned by
lpa-studio-ux
        consumed by
lpa-studio-web, future CLI, desktop, and agents
```

`Ux` means a resource-owning product surface. It owns lower-level services such
as `lpa-link` and `lpa-client`, then exposes user-shaped state, snapshots,
typed actions, contextual action metadata, progress, issues, logs, and project
summaries.

The first implementation slice uses:

- `StudioUx` as the top-level surface;
- `LinkUx`, `ServerUx`, and `ProjectUx` as domain sub-surfaces;
- `StudioSnapshot` and related snapshots as cloneable read models;
- typed commands such as `LinkAction` and `ProjectAction`;
- `AvailableAction<A>` to attach contextual labels, summaries, priorities, and
  enablement to typed commands;
- async `execute()` methods that perform the real operation and update the UX
  state directly.

The first proof path is browser-worker simulation only. `lpa-studio-ux` owns the
browser-worker provider/session and adapts it into `lpa-client::LpClient<Io>`.
The web app renders snapshots and dispatches actions; it does not route runtime
providers, drain service effects, or correlate protocol responses.

The older `lpa-studio-core` and `lpa-studio-runtime` crates remain in the
workspace as compiling references during the experiment. A later cleanup can
delete, archive, or fold them once the new model proves itself.

## Consequences

- Studio behavior becomes easier to inspect through states and available
  actions.
- Web UI, future CLI, tests, and agents can share the same action/snapshot
  language.
- Service operations move out of the UI and out of an abstract effect/event
  loop.
- The first slice is smaller than the old provisioning UI; Web Serial ESP32,
  flashing, and rich recovery states must be ported intentionally later.
- The old ADRs and crate READMEs may describe reference code rather than the
  active direction until cleanup is complete.

## Alternatives Considered

- Keep the `lpa-studio-core` / `lpa-studio-runtime` split.
  - Rejected for the active direction because it made real application
    ownership look like external effects and still leaked runtime composition
    into the web app.
- Rename the old crates to backup directories immediately.
  - Rejected for the experiment because keeping them compiling as references
    makes the slice easier to compare, inspect, and discard.
- Start with a generic UX component tree.
  - Deferred. Domain-specific `LinkUx`, `ServerUx`, and `ProjectUx` states are
    clearer for this stage. Generic view concepts can emerge from repeated
    needs.

## Follow-Ups

- Port browser serial ESP32 and firmware flashing into the UX model.
- Decide whether `lpa-studio-core` / `lpa-studio-runtime` should be deleted,
  archived, or folded after the experiment.
- Add a CLI or test harness that drives `StudioUx` directly.
- Rebuild richer Studio visual stories on the new snapshot/action model.
