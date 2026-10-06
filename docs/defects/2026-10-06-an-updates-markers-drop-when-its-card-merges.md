---
status: fixed
found: 2026-10-06      # how: e2e (`just walk-ota-emu`, step `cant-get`)
fixed: fa7a99e7e
area: lpa-devices roster.rs (reconcile_identities, marker routing); lpa-studio-core update_host.rs (follow_merge)
class: lifecycle-ownership
related:
  - docs/adr/2026-10-06-studio-updates-over-the-update-channel.md (decision 3)
  - docs/defects/2026-10-06-an-identify-clears-how-an-update-ended.md (same step)
---
# An update's markers drop when its card merges into a remembered board

**Symptom** — Walk step `cant-get` (E13): a core-only board was kept with
"Set up this device", then "Install dev …" was pressed. The board took the
engine and said hello. The card then sat on "Finishing the update… 0%" for
good.

**Root cause** — The board's hello named an identity this browser already
remembered, so identity reconciliation folded the kept card into the
remembered record and removed the kept card's id. The Update activity
moved with the card. But the update host's run, its tick loop and every
marker it sent were still addressed to the old id, and the roster drops
markers for an id it no longer holds.

**Fix** — The roster records each merge (`Roster::merged_into`) and routes
a marker addressed to a merged-away id to the surviving one. The update
host re-keys its run, its pins and its ticks to the surviving id
(`UpdateHost::follow_merge`).

**Regression coverage** —
`studio_update_e2e_tests::an_install_on_a_kept_card_that_merges_into_a_remembered_board_ends_on_y`,
and the walk's `cant-get` step.

**Lesson** — Anything long-running that is keyed by `DeviceId` must survive
the roster changing a device's id under it. A merge is a lifecycle event
for every owner of that id, not just for the card.
