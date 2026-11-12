---
status: fixed
found: 2026-07-17      # how: hardware-walk
fixed: 113c66420
area: lpa-studio-core/places
class: assumed-context
related: []
---
# Pull/push hardcoded the sim's storage slot instead of asking the device

**Symptom** — A device visibly running a project was detected as
"Connected — nothing loaded", and push wrote the project into a
directory the device wasn't running from — the running project and the
pushed copy lived in different dirs, neither aware of the other.

**Root cause** — `pull_device_copy` and push both hardcoded the storage
slot `"studio"` — the slot the sim happens to use. Devices provisioned
via the CLI run from other directories under `/projects/`. The client
never asked the device where its loaded project lived; it assumed the
answer.

**Fix** — `pull_device_copy` discovers the *loaded* project's storage
dir via `list_loaded_projects` and reads from there; push replaces the
project in place, in the dir it actually occupies; `PulledDeviceCopy`
carries the discovered `storage_id` so downstream flows keep targeting
the right slot. As part of the same change, the edit-e2e device
fixtures were made honest — they had been secretly running the edit
project, which is why the assumption never failed in tests.

**Regression coverage** — e2e
`device_running_from_a_non_default_storage_dir_classifies_not_empty`.

**Lesson** — Ask the device what it has. Every client-side assumption
about device state — which slot, which dir, which project — is a bug
waiting for a provisioning path that violates it, and provisioning
paths multiply. The source of truth for on-device state is the device;
anything else is a cache of a guess.
