---
status: fixed
found: 2026-07-23      # how: report
fixed: 384563337
area: lpa-studio-core/device (deploy_session + studio_controller)
class: assumed-context
related:
  - 2026-07-17-hardware-attach-opened-editor.md
  - Planning/lp2025/2026-07-20-runtime-pool (P2)
---
# Deploy dialog opened on the picker while the device already ran a known project

**Symptom** — Opening the deploy dialog from a device card with no
explicit target (`DeployOp::OpenDialog { target_key: None }`) always
landed on `ChoosingPackage`, even when the connect-time pull had already
classified the device as running a library-known project
(`DeviceContent::Known` / `Adopted`). The user, looking at a card that
says "porch — behind your copy", clicks Push and is asked *which
project?* — a question the studio already knew the answer to.

**Root cause** — `derive_state` treats "no target chosen" as "nothing
known about intent": the target only ever came from the caller's
`target_key`. But the environment snapshot the dialog derives from
(`DeployEnvironment.device_sync`) already carries what the device runs.
The dialog assumed the picker context instead of asking the source of
truth it was literally holding — the classified device content.

**Fix** — `StudioController::execute_deploy_op(OpenDialog)`: with no
explicit `target_key`, a device whose sync content is `Known`/`Adopted`
gets that project resolved via `resolve_deploy_target` and passed as the
open's pre-target, so entry derives `Reviewing` (the honest default). A
failed resolve (project since deleted, no library) falls back to the
picker. Choosing a different project remains reachable — `choose_target`
already accepts `Reviewing`, so the review step's picker still swaps
targets; the default removes a question, not a choice.

**Regression coverage** —
`deploy_session::tests::pre_targeted_open_reviews_and_a_different_choice_stays_reachable`
(state machine) and
`studio_link_e2e_tests::deploy_dialog_pre_targets_the_running_project`
(full link path: real pull classification feeding the pre-target).

**Lesson** — When a dialog's entry state is a *derivation*, every input
the derivation could honestly use should flow into it; leaving a knob
for the caller ("optional preselect") invites the assumption that
absence of the knob means absence of knowledge. Defaults should be
computed from observed state, and "you can still choose otherwise" is
the test for whether an honest default is safe.
