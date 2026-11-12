---
status: fixed
found: 2026-09-07      # host e2e, PR #580 (plan P2: the sim as a roster device)
fixed: this change
area: lp-app/lpa-studio-core/src/app/studio/studio_controller.rs (settle_device_records)
class: write-ordering
related: [lp-app/lpa-devices/src/roster.rs (reconcile_identities), lp-app/lpa-studio-core/src/app/devices/device_frame_snapshot.rs, lp2025/2026-09-07-0118-studio-emulated-boards]
---
# A device merge deleted the registry row the same batch had just written — and its sidecars with it

**Symptom** — A remembered device whose link comes back loses its registry
row. Found on a sim (a created record, powered on: the row and the
`/device-sims/<uid>.json` sidecar were both gone by the time the card
reached Ready, so the device stopped being a sim and stopped being
remembered), but the mechanism is not sim-specific: any remembered board
that re-attaches and re-identifies takes the same path.

**Root cause** — `Roster::reconcile_identities`
(`lpa-devices/src/roster.rs`) merges two entries that turn out to be one
device and emits, in ONE batch, `DeleteRecord(discarded_handle)` followed
by `PersistRecord(surviving_record)`. Both resolve to the **same registry
key** — being the same device is exactly what the merge discovered. The
model is right: it is naming a handle, not a device, and the row key is
identity, not handle.

`settle_device_records` performed every persist first and every delete
after, so the delete landed on the row the same batch had just written.
For a serial board the damage looked transient — the next heartbeat
persisted the record again and the row came back — which is why nobody had
seen it. The damage that did NOT come back was the sidecars:
`CatalogOp::ForgetRegisteredDevice` deletes the device's
`/device-frames/<uid>.json` with the row, so a remembered board silently
lost its last picture every time its port returned. With the sim record
added beside it, the same delete took away the file that says a device is
a sim, which is what made the bug visible.

**Fix** — `settle_device_records` records the uids the batch persisted and
skips any delete resolving to one of them, logging it as the merge it is.
A delete for a device the batch did not also write (a real `Forget`) is
unaffected. Covered by the P2 e2e rows: powering a sim on keeps its row and
its sidecar, and forgetting it takes both.
