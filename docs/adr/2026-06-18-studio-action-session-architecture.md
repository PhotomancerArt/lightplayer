# ADR: Studio Action And Session Architecture

Date: 2026-06-18

## Status

Superseded by [2026-06-21 Studio UX Layer](./2026-06-21-studio-ux-layer.md)

## Context

LightPlayer Studio needs a real UI while preserving the product's core embedded
GLSL JIT architecture. The first Studio milestone also needs to stay honest for
future non-UI consumers: host harnesses, tests, and agents should be able to
drive the same product surface as the web UI.

The first implementation slice uses:

- `browser-worker` for static web/GitHub Pages-style simulation.
- `host-process` for host-side non-UI harness validation.
- `lpa-link` below Studio for endpoint discovery, status, management, logs,
  diagnostics, and opening server/client connections.

At the same time, Studio actions need to look forward to generic UI help,
agent-discoverable tools, and undo/redo, without implementing editing or undo in
M1.

## Decision

Studio is split into three application-facing crates:

- `lp-studio-core` owns state, documented actions, action metadata, effects,
  events, diagnostics, capabilities, and session records.
- `lp-studio-runtime` executes effects and translates link/client/runtime facts
  back into Studio events.
- `lp-studio-web` renders `StudioState` with Dioxus and dispatches
  `StudioAction` values.

The core loop is:

```text
StudioAction -> StudioState + StudioEffect -> StudioEvent -> StudioState
```

The core crate is synchronous and UI-free. It does not perform I/O, spawn
runtimes, own browser workers, open serial ports, or render components.

Runtime code owns I/O. It consumes `StudioEffect` values and emits
`StudioEvent` values. The host path validates this with `host-process` and
`fw-host`; the browser path validates it with the `fw-browser` Web Worker
envelope.

Actions are documented program objects. Each action has metadata and a
descriptor surface for labels, summaries, categories, origin, correlation, and
history policy. This gives UI controls and future agents a common way to explain
and inspect available operations.

M1 does not implement undo/redo. It only classifies history behavior. Most M1
actions are operational, read-only, or navigation actions, so they are
non-undoable or ephemeral. Future global undo should attach to successful domain
edit transactions, not to every `StudioAction`.

## Consequences

- Dioxus is a thin consumer rather than the owner of Studio behavior.
- A non-UI harness can validate the same action/effect/event shape as the web
  UI.
- The first deployed web path can use `browser-worker` without implying that
  browser simulation replaces ESP32 runtime compilation.
- `lpa-link` remains below Studio capabilities; Studio does not branch UI code
  directly on serial/process/worker mechanics.
- Future agents can discover documented actions and capabilities without
  scraping UI components.
- Undo remains deliberately deferred, but the action model will not need to be
  reclassified later to separate operational actions from edit history.

## Deferred

- Real ESP32 hardware UX and Web Serial belong to M2.
- Agent execution/harness depth belongs to M3.
- Editing, overlay conflicts, commit/discard, undo/redo, and file/body edit
  sessions belong to later authoring milestones.
