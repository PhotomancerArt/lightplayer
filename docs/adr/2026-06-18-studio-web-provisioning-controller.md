# ADR: Studio Web Provisioning Controller

- **Status:** Superseded by [2026-06-21 Studio UX Layer](./2026-06-21-studio-ux-layer.md)
- **Date:** 2026-06-18
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** [2026-06-21 Studio UX Layer](./2026-06-21-studio-ux-layer.md)

## Context

The first Studio web UI proved the end-to-end browser and hardware paths, but
it did so through demo helper functions that returned a fully completed
`StudioApp`. That was useful for proof-of-concept work, but it hid the product
journey from the UI: provider catalog, permission/access, endpoint discovery,
link opening, server readiness, project-state inspection, project selection,
recovery, deploying, and readiness.

Studio now has a core provisioning model and deterministic scenario runtime.
The web app needs to drive that model from day one without moving domain state
into Dioxus-local wizard code. It also needs to keep simulator and Web Serial
hardware paths shaped the same way, because future host/web providers should
plug into the same user journey.

## Decision

Use a thin browser-side provisioning controller in `lp-studio-web`.

- The controller dispatches real `StudioActionKind` values into `StudioApp`.
- It drains returned `StudioEffect` values through the active browser runtime.
- It applies returned `StudioEvent` values back into `StudioApp`.
- It owns browser-provider routing, including a merged provider catalog for
  browser-worker simulator and browser-serial ESP32.
- It auto-advances only obvious steps, such as endpoint granted -> connect and
  server ready -> read project state.
- It does not own Studio domain transitions, project semantics, provider
  availability, or recovery decisions.
- Browser-worker is a real `EffectExecutor`, like browser serial and host
  process, rather than a bespoke helper path.
- After server readiness, the default path is `ReadProjectState`: attach to one
  loaded project, ask for user/project intent when zero or many are loaded, and
  render recovery when recovery is reported. Loading the starter project is an
  explicit user action.

## Consequences

The main web app renders the provisioning journey from shared Studio state
instead of swapping in a finished demo app.

Simulator and browser-serial hardware now pressure the same
action/effect/event/reducer contract. Future browser flashing, host serial, and
remote providers can reuse the same controller shape with provider-specific
capabilities.

The controller is intentionally thin and browser-specific. It is not a new
domain layer, and it should not grow product policy that belongs in
`lp-studio-core` or protocol/runtime behavior that belongs in
`lp-studio-runtime`.

Story fixtures and PNG baselines can show the user journey state by state. The
visual story set becomes a lightweight review surface for this foundational UX.

## Alternatives Considered

- Keep using demo helpers that return completed `StudioApp` values.
  - Rejected because the real onboarding/provisioning journey would remain
    invisible to the UI and hard to review.
- Put the provisioning controller in `lp-studio-core`.
  - Rejected because core must stay UI- and runtime-independent.
- Put the controller in `lp-studio-runtime`.
  - Rejected because provider routing and Dioxus signal/update concerns are web
    presentation concerns; runtime adapters should execute effects.
- Upload/load the starter project automatically after connecting.
  - Rejected because real hardware usually already has a project loaded, and
    overwriting/loading should be explicit user intent.

## Follow-ups

- Wire real browser-side ESP32 flashing into the same provider/controller flow.
- Add real recovery/safe-mode protocol once firmware/server support exists.
- Consider extracting shared controller test helpers if desktop Studio needs
  the same auto-advance policy outside Dioxus.
