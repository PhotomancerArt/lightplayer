---
status: fixed
found: 2026-10-06      # how: e2e (`just walk-ota-ble-emu`'s first runs, OTA M7 P12)
fixed: this change
area: lpa-devices `UpdateActivity` (the gap's knock) × `Roster::attach_link` (re-attach by endpoint)
class: assumed-context
related:
  - docs/adr/2026-10-06-studio-updates-over-the-update-channel.md
  - docs/defects/2026-10-06-an-update-hangs-when-the-boards-reset-keeps-the-port-open.md
  - lp2025/2026-10-05-0820-ota-studio-ble-updates (P3, P12)
---
# A Bluetooth update never opens the link its reconnect attaches

**Symptom** — over `?ble=emu`, Studio backed the board up, the board reset
into core-only, and the update stopped there. The card read "Attached — not
listening", "No response — try flashing firmware", and its terminal's last
line was "reconnecting — the board's link closed". Ninety seconds later the
update ended `BoardDidNotComeBack`. The polyfill showed the GATT connection
back up (`connects: 2`) and the board sending; Studio wrote only keepalives.

**Root cause** — a board reset over Bluetooth is a GATT drop. The departure
sweep detaches the dropped link, the provider's reconnect loop brings the
device back, and the connect edge's sweep attaches a NEW link — closed. The
roster re-attaches it by endpoint through `fold_only` and then
`spawn_identify`, and Identify is what opens a re-attached link; on a device
the Update activity holds, `spawn_identify` is a no-op. The activity's gap
knocks only `ClosedPort::Wait` on a Bluetooth link (P3: a connect on the
dropped link would fight the provider's loop), so nothing ever opened the
new one. The core-only board's `M` on channel 3 sat unread, and the gap ran
out. Over USB a re-enumerated port is reopened by the knock, so P9's walk
never saw it; the e2e test's Bluetooth double reopened its own link, so it
did not either.

**Fix** — the gap's knock opens a closed Bluetooth link whose presence began
AFTER the gap did (`UpdateActivity::closed_port`): that is the reconnect's
new link, its session already connected, so `Open` is the model starting to
listen. The dropped link's own close is never after the gap began (the leg
ends on it), so it is still left to the provider's loop.

**Regression coverage** — `lpa-devices`:
`a_bluetooth_link_attached_between_legs_is_opened`,
`a_dropped_bluetooth_link_is_left_to_the_providers_loop`;
`lpa-studio-core`: `an_update_over_bluetooth_comes_back_by_itself_across_each_reset_and_ends_on_y`,
whose board double now plays the provider's departure and return (a new,
closed link) and fails without the fix; `just walk-ota-ble-emu`.

**Lesson** — an activity that outlives its link inherits every job the
idle device does on a new link. Re-attach opened links only by spawning
Identify, which a busy device refuses, so the busy path had no opener. A
test double that brings its own link back open skips the step that broke.
