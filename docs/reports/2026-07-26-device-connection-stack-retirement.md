# Retiring the device "connection-stack" pane surface

Date: 2026-07-26
Branch: `claude/device-lifecycle` (this work sits on top of the in-flight
device-lifecycle milestone; based on tip `fd441363d`)

## Why

The device UX was rebuilt across M6 (auto-connect + retry ladder), M7′
(card-as-control-panel) and M8′ (provisioning-on-card, deploy-dialog
deleted). Home is the gallery of device cards; the editor docks the D43
"grown" `DeviceCard` as its right-side pane. The OLD surface — a step
stack titled "Device" with *Select connection → Connect device → Connect
LightPlayer → Open project* — survived only as a defensive fallback. This
change proves it was unreachable and removes it.

## Reachability analysis (what was reachable vs dead)

The step-stack pane reached the screen through exactly one branch in the
web shell (`studio_shell.rs`):

```
if let Some(card) = lens_card { DeviceCard { … } }
else if let Some(device) = device { PaneView { … } }   // the dead branch
```

Tracing the controller (`lpa-studio-core`):

1. `StudioController::view()` **early-returns with empty `panes`** whenever
   `home_view()` is `Some` — i.e. whenever no project is loaded
   (`home_view()` returns `None` only when `project_is_loaded()`). So the
   device pane could only ever be emitted while a project is loaded.
2. A project can only reach `ProjectState::Ready` through
   `connect_running_project[_if_available]`, and **both resolve their
   server via `self.pool.lens_session_mut()?`** — they error out before
   the project is marked `Ready` if there is no lens session. Therefore
   **project loaded ⟹ a lens session exists**.
3. `lens_card = lens_device_card()` yields `Some` whenever a lens session
   exists:
   - device lens ⟹ `pool.device_session()` is `Some` ⟹ `device_state()`
     is `Some` ⟹ the roster evidence's `link` is non-`None` ⟹
     `live_device_card()` returns a card;
   - sim lens ⟹ `pool.sim_session()` is `Some` ⟹ `evidence.sim` is
     `Some` ⟹ `sim_card()` returns a card.

So whenever `panes` contained the device pane (project loaded), `lens_card`
was always `Some`, and the `else if let Some(device)` branch was
**unreachable**. The comment already suspected this ("the editor implies a
lens"); the trace confirms it.

Corollary: `StudioController::actions()` / `view_actions()` (which
enumerate actions off `view.panes`) has **no production consumer** — only
tests referenced it. The live device affordances (provision / erase /
reset / disconnect / push) are wired directly on the card component
(`lpa-studio-web/.../home/device_card.rs`) and covered by
`studio_link_e2e_tests.rs`, not by this enumeration.

## What was removed

Core (`lpa-studio-core`):
- `DeviceController::view()` and its private helpers `status`,
  `disconnected_device_section`, `connected_device_section`,
  `connected_device_actions`, `firmware_section`, plus the free fn
  `content_line` and the `SECTION_FIRMWARE` const and the
  `DeviceRuntimeEvidence` struct/impl (all only fed `view()`).
- `StudioController`: stopped emitting `device_view` into the `panes` vec
  (now `[project, bus]`); removed the now-orphaned helpers
  `device_runtime_evidence`, `usual_device_line`, `is_hardware_attached`.
- Tests: deleted the four device-pane tests
  (`incompatible_device_surfaces_reflash_affordance_in_the_pane`,
  `no_firmware_marks_the_device_pane_ready_to_flash`,
  `device_pane_offers_firmware_ops_separately`,
  `loaded_project_keeps_management_recovery_actions_visible`) and the
  vacuous `initial_actions_target_device_node`; rewrote
  `loaded_project_gets_project_pane` for the 2-pane reality; dropped the
  now-unused `device_section_ids` helper.

Web (`lpa-studio-web`):
- `studio_shell.rs`: removed the dead `else if let Some(device)` branch,
  the `PaneGroups`/`group_panes` device split, `device_is_primary`, and
  the `DeviceController` import. The right column now only ever renders the
  lens card.
- Deleted `app/device/device_pane_stories.rs` (+ the empty `app/device`
  module) — all six stories depicted the retired step-stack pane.
- Deleted `app/layout/studio_shell_stories.rs` — all five stories
  (`simulator_idle/endpoint/starting/ready`, `action_error`) were the
  retired "simulator connection" framing; whole-shell rendering stays
  covered by `project_workspace_stories` (which drives the real
  `StudioShell`), and the sim states are covered by `roster_card_stories`.
- `project_workspace_stories.rs`: deleted `device_project_empty` and
  `device_project_selection` (step-stack "Open project" fixtures).
- `story_fixtures.rs`: removed the connection-stack fixtures
  (`device_view`, `idle/endpoint/starting/simulator_ready_device_view`,
  `device_project_empty/selection_view`, `picker_issue_view`, `error_view`,
  `idle/endpoint/starting/simulator_ready_view`, `stack_section`,
  `select_connection_complete`, `connect_device_complete[_with_actions]`,
  `browser_worker_metrics`, `esp32_metrics`, `start_actions`,
  `connected_esp32_recovery_actions`, `disconnect_device_action`,
  `disconnect_lightplayer_action`, `device_action`, `project_action`) and
  dropped the device pane from the still-live `project_ready_view`,
  `project_syncing_view`, `project_sync_failed_view`.
- `story_registry.rs`: repointed `DEFAULT_STORY_ID` from the deleted
  `studio/layout/studio-shell/simulator-idle` to
  `studio/home/home-gallery/populated` (the primary current surface).

Story count dropped 278 → 266 (12 `#[story]` fns removed).

## What stays and why

- The generic `UiViewContent::Stack` / `UiStepsView` **view primitive** is
  untouched — it is a core widget with its own widget stories
  (`core/view/steps_view_stories`, `view_content_stories`); the device
  surface was merely one consumer.
- `DeviceController::SECTION_DEVICE` stays — it is the activity/overlay
  routing target for connect/flash/push narration, independent of the
  retired view.
- All `DeviceOp` variants and their handlers stay — they are the live
  card's ops.
- The project-editor fixtures (`project_synced_pane_view`,
  `project_editor_fixture`, `project_workspace_nodes`, …) and their live
  stories stay.

## Story baselines

Orphaned baselines for the deleted stories were removed (42 PNGs:
`studio__device__device-pane__*`, `studio__layout__studio-shell__*`,
`studio__project__project-workspace__device-project-*`). This matches what
a canonical `just studio-story-baselines` run would prune.

The **12 changed** baselines (`project-workspace__{project-ready,
project-syncing,project-sync-failed,overview}` × 3) still need a canonical
refresh — dropping the device pane changes those renders. They were **not**
regenerated here: a local capture in this environment produced **0 / 783
byte-identical** PNGs against the committed set (all ~1.5× larger),
confirming the committed baselines were captured in a different environment
(the documented local-vs-CI capture sensitivity —
`docs/debt/story-capture-pipeline.md`). Regenerating from here would
replace 783 correctly-styled baselines with environment-drifted ones.
**Action for the canonical capture environment:** run
`just studio-story-baselines` (the whole set appears environment-drifted on
this branch, not just these 12) and review via the usual visual gate.

## Incidental

- Pre-existing rustfmt drift in `home/device_card.rs` (a multi-line
  `ConfirmSheet` signature the pinned rustfmt collapses) was the only file
  `fmt-check` flagged; normalized so the gate passes. Unrelated to the
  retirement.

## Gates

- `just check` — green.
- `just test` — green (after `scripts/build-builtins.sh`, a pre-existing
  setup step for the RV32 shader filetests; unrelated to this change).
- `lpa-studio-core`: 557 tests pass, including the `studio_link_e2e_tests`
  that exercise the live card provision/reflash/push flows.
