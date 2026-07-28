---
status: open
found: 2026-07-28      # how: audit (README hero story showed the retired device UI)
area: lpa-studio-core/home (lens_device_card) + lpa-studio-web/app/layout/studio_shell
class: retired-surface-still-reachable
related:
  - ../adr/2026-07-05-studio-pane-grammar.md
---
# Unplugging a device mid-project falls back to the retired device pane

**Symptom** — With a project open on a USB device, unplugging the cable
leaves the editor's right column rendering the RETIRED step-stack device
pane ("Select connection / Connect device / Connect LightPlayer"), not the
D43 lens card. The state persists until the user acts; it is not a
one-tick transient.

**Root cause** — Two independent facts meet:

1. `live_device_card` returns `None` when the derived roster state is
   `Offline` (`lpa-studio-core/src/app/home/home_view_builder.rs:362-364`),
   and `derive_roster_card_state` maps `Some(DeviceState::Gone)` → `Offline`
   whenever `connect == Idle` (`app/roster/roster_evidence.rs:100-128`). So
   `lens_device_card()` (`app/studio/studio_controller.rs:661-676`) goes
   `None` the moment the lens device reports `Gone`.
2. Nothing resets the project or detaches the lens on device-gone. The web
   hotplug `disconnect` listener sends only a `RefreshTick`
   (`lpa-studio-web/src/web_app.rs:490-513`); the only non-test reaction to
   `DeviceState::Gone` in the controller is `connect_server_from_link`
   (`studio_controller.rs:3004`), which runs on a *user-initiated*
   reconnect. `project.state` stays `Ready`, so `home_view()` stays `None`
   and the pane layout keeps rendering.

The shell then falls through `lens_card` → `PaneView { view: device }`
(`lpa-studio-web/src/app/layout/studio_shell.rs:121-142`). The comment
there calling that branch "defensive: the editor implies a lens" is wrong.

A second, narrower path: `settle_connect_outcome`
(`studio_controller.rs:2185, 2189, 2201, 2206`) clears the lens via
`remove_kind` without the `project.reset()` its sibling teardown paths
pair with — a soft-ended connect on the lens's own runtime kind leaves the
project `Ready` with no lens.

**Why it went unnoticed** — `view.lens_card` has zero test assertions in
`lpa-studio-core`; no test pins "panes non-empty ⇒ lens_card is Some". The
device-pane tests that do exist all assert on the core DTO
(`studio_controller.rs:4647, 4975, 5000, 5028`), a surface the shipped
shell discards whenever a lens card exists.

**Decision (Yona, 2026-07-28)** — Make `lens_device_card()` fall back to an
offline card rather than returning `None`, so the DeviceCard is the only
device surface in every state. That closes the hole and unblocks deleting
the retired pane (~700 lines across core + web, ~54 baselines). The
deletion's real cost is retargeting the 16 production
`UxActivityTarget::StackSection` call sites that narrate connect/flash
progress into the pane. Scheduled as its own PR after #163.
