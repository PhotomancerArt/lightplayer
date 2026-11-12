---
status: fixed
found: 2026-07-17      # how: hardware-walk
fixed: a5c7a5231
area: lpa-studio-core/studio
class: policy-leak
related: []
---
# Hardware attach ran the sim's running-project probe, opened the editor

**Symptom** — Connecting a board that had auto-resumed its startup
project jumped straight into the editor, which sat on "Waiting for
project data" forever. The gallery — where a hardware connect is
supposed to land — was skipped entirely.

**Root cause** — `attach_runtime` ran the *sim's* running-project probe
(`connect_running_project_if_available`) on hardware attaches too. The
policy is a sim policy: when the sim is running a project, Studio
should jump into editing it. It only became visible on hardware once
the device-standalone fixes landed auto-resume, making
`list_loaded_projects` non-empty on a real device for the first time.
The fake device never auto-loaded a project, so the e2e suite was
structurally blind to the branch.

**Fix** — Hardware attaches observe only: `probe_server_readiness`
drives the readiness state without ever connecting the editor. The
sim keeps its jump-into-editor behavior. The fake device gained
`with_loaded_project()` so a test device can boot in the auto-resumed
state, closing the blind spot.

**Regression coverage** — e2e
`attaching_a_device_with_a_loaded_project_never_opens_the_editor`,
verified failing against the old behavior before the fix.

**Lesson** — Policy branches keyed on runtime *kind* must be explicit —
a shared attach path that silently applies one kind's policy to all
kinds is a leak with a fuse, lit whenever the other kind's state space
grows. And test doubles must mirror real-hardware boot behavior: a fake
that can't reach the state the real device boots into makes the
divergence untestable, which is how this one shipped.
