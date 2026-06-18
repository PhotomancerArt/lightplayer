# ADR: Studio Provisioning Core Model

- **Status:** Accepted
- **Date:** 2026-06-18
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None

## Context

The first Studio UI proved that LightPlayer can run in the browser, connect to
the browser worker simulator, connect to ESP32 hardware over Web Serial, upload
a demo project, and show basic runtime state. That UI was intentionally
demo-shaped: a pair of buttons drove scripted helper flows and the app inferred
device state from low-level session fields.

The real Studio needs a product provisioning flow. A user should be able to
open Studio, choose a simulator or hardware provider, handle browser/runtime
availability, grant permission, open a link, probe the target, recover or flash
when needed, deploy/load a project, and arrive at a live project session. The
same flow also needs to be understandable by future agents and deterministic
fake runtimes.

`lpa-link` already owns lower-level provider, endpoint, session, connection, and
management vocabulary. It should not grow product UX concepts such as "show a
compatible browser help action" or "this provisioning issue can be retried."

## Decision

Add a first-class provisioning/device-manager model to `lp-studio-core`.

- `DeviceManagerState` is part of `StudioState`.
- `ProviderCatalog` owns provider cards, selected provider, provider
  availability, and discovered/granted endpoints.
- `DeviceFlowState` models the user journey through choosing a provider,
  requesting access, opening a link, probing, provisioning, flashing, deploying
  a project, readiness, degradation, and disconnection.
- `DeviceIssue` and `RecoveryAction` provide structured failures and suggested
  next steps for UI and future agents.
- `ProgressState`, `ProvisioningReason`, and `TargetProbeResult` model
  long-running operations and target classification even before real flashing
  is implemented.
- Existing live records such as `DeviceSession`, `ConnectionSession`,
  `ClientSession`, and `ProjectSession` remain canonical for the connected
  runtime and loaded project.
- `LinkSelection` is removed as canonical Studio state. Provider selection and
  endpoints now live in `ProviderCatalog`.
- Actions, effects, and events gain provisioning vocabulary for provider catalog
  refresh, starting/canceling/retrying provisioning, target probing, progress,
  and typed issues.

## Consequences

The web UI can render the provisioning journey directly from core state instead
of keeping hidden wizard state in Dioxus.

Runtime implementations and future fake scenarios can emit incremental events
for intermediate states rather than returning a fully scripted `StudioApp`.

Future agents can inspect the same documented actions, issues, and recovery
actions as the UI.

`lpa-link` stays focused on low-level link/device/runtime concerns. Product
availability and recovery guidance stay in Studio core.

There is temporary scaffold churn in current demo helpers and stories because
they must read provider selection/endpoints from `ProviderCatalog` instead of
`LinkSelection`. That churn is accepted because the demo UI is not the product
architecture.

## Alternatives Considered

- Keep deriving provisioning state from optional session fields.
  - Rejected because states such as unsupported browser, permission canceled,
    blank device, flash needed, or project deploy failed cannot be represented
    cleanly by `Option<DeviceSession>` and `Option<ProjectSession>`.
- Keep `LinkSelection` for compatibility with demo helpers.
  - Rejected because the references were only scaffolding pressure. In active
    development, preserving stale state would make the final model less clear.
- Move product availability and recovery guidance into `lpa-link`.
  - Rejected because `lpa-link` should remain below Studio product semantics.
- Build the final Dioxus provisioning UI first.
  - Rejected because the UI should render a tested core flow instead of
    inventing local wizard state.

## Follow-ups

- M2 provisioning work should add deterministic scenario fakes and flow tests.
- M3 should replace the demo UI with the real onboarding/device-manager UX and
  journey stories.
- Real browser/host ESP32 flashing and recovery should plug into the same flow
  model instead of creating a separate provisioning path.
