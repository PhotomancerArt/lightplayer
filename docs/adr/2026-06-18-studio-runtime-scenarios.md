# ADR: Studio Runtime Scenarios

- **Status:** Superseded by [2026-06-21 Studio UX Layer](./2026-06-21-studio-ux-layer.md)
- **Date:** 2026-06-18
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** [2026-06-21 Studio UX Layer](./2026-06-21-studio-ux-layer.md)

## Context

The Studio provisioning core model gives the UI, runtimes, and future agents a
shared vocabulary for provider choice, permissions, link opening, target
probing, flashing, project loading, readiness, degradation, and recovery.

That model needs deterministic coverage before the real Dioxus provisioning UI
is built. Hardware and browser APIs are useful integration targets, but they are
too manual and environment-dependent to cover the full matrix of product states:
unsupported browser, permission cancellation, blank device, endpoint-open
failure, firmware incompatibility, flash failure, project deploy/load failure,
and connection loss.

`lpa-link` already has lower-level fake providers, but those fakes model link
provider mechanics. They should not become the owner of Studio product outcomes
or UI journey states.

## Decision

Add a public, I/O-free `lp-studio-runtime::scenario` module for deterministic
Studio provisioning scenarios.

- Scenario definitions live in `lp-studio-runtime`, not `lp-studio-core` and
  not `lpa-link`.
- `ProvisioningScenario` describes product-level runtime outcomes.
- `ScenarioRuntime` implements `EffectExecutor` and maps scripted outcomes into
  real `StudioEvent` values.
- `ScenarioHarness` drives a real `StudioApp` through real
  `StudioActionKind -> StudioEffect -> StudioEvent -> StudioApp::apply_event`
  loops.
- `ScenarioSnapshot` records lightweight journey checkpoints for tests and
  future UI stories.
- Scenario data has no browser, host-process, hardware, timing, or filesystem
  dependency.

## Consequences

Provisioning tests can cover happy and failure paths without Web Serial,
firmware, device availability, or manual interaction.

Future Dioxus journey stories can reuse the same scenario vocabulary instead of
inventing separate UI fixture states.

The scenario layer pressures the real core reducer and event contract. When a
user-visible path cannot be expressed, the core model must gain a narrow typed
event or status instead of hiding the path behind a generic runtime error.

`lpa-link` remains focused on provider/endpoint/session/management mechanics.
Studio product outcomes such as permission canceled, flash failed, or project
deploy failed stay above that layer.

The public runtime test fixture surface is now part of Studio architecture. It
should stay small, serializable where useful, and I/O-free.

## Alternatives Considered

- Keep scenario fakes as private test helpers.
  - Rejected because M3 UI journey stories and future agent harnesses need the
    same vocabulary.
- Put the fakes in `lp-studio-core`.
  - Rejected because scenarios execute effects and model runtime outcomes; core
    should own state transitions, not fake effect execution.
- Put the fakes in `lpa-link`.
  - Rejected because the important states are Studio product outcomes rather
    than link-layer mechanics.
- Rely on browser and hardware integration tests only.
  - Rejected because they cannot cheaply or deterministically cover permission,
    flashing, project, and connection failure combinations.

## Follow-ups

- M3 should render provisioning journey stories from `ProvisioningScenario` or
  `ScenarioSnapshot` rather than duplicating fixture vocabulary in the web app.
- Real browser/host flashing work should emit the same typed events and issues
  exercised by scenario tests.

## Update 2026-06-22

The `lpa-studio-runtime` crate was deleted when Studio moved to the
resource-owning `lpa-studio-ux` model. Future deterministic Studio stories and
tests should be built from `StudioView`, typed `UxAction` values, and focused
UX-node fixtures rather than the deleted runtime scenario module.
