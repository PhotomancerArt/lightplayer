---
status: fixed
found: 2026-10-03      # hardware-walk (C6 repartition G1, scene 1, the spare XIAO C6)
fixed: this change
area: lpa-studio-core access (device_access_ops::sync_access, AccessController)
class: partial-knowledge-loss
related:
  - lp2025/2026-10-01-1843-c6-repartition (PR #896, G1)
  - docs/adr/2026-09-24-ble-transport.md
---
# A full device store loses the list Studio read: "Who has access 0", Bluetooth switch locked

**Symptom** — G1 scene 1 on the spare XIAO C6 (old layout, #891 firmware,
`/.lp/access.json` holding 16 entries — the cap — with `open: true` and
`bleEnabled: true`): the layout migration passed, but in the device's access
panel the **Bluetooth switch was greyed out** and could not be turned on. An
earlier headless run of the same migration (a fresh browser profile) showed
"Who has access 0" right after the update, while the board itself answered
`AccessList` with its entries.

**Root cause** — Studio's USB sync (`sync_access`, run once per connection)
reads the board's list, then adds the keys this browser holds that the
board lacks. On a store already at `MAX_SECRETS_PER_FILE` (16) the board
refuses the add (`cannot add access: 17 secrets in one access file (at most
16)`), and the sync's `?` returned that refusal as the WHOLE result — the
list it had just read went with it. `AccessCommand::Synced(Err)` was silent
by design ("the panel still shows what it last knew"), so a browser with no
cached record had nothing: the panel's `ble_enabled` stayed `None`, which
the web reads as "still reading" — `Reading the device's list…`, a count of
0, and both switches locked (`connections_group::bluetooth_row`,
`device_access_panel`). A browser with an older cached record kept showing
that record, never refreshed, for every connect after the store filled.
The migration was not the cause: the same board, connected and never
updated, fails the same way (the e2e test below fails at its first
assertion, before any update). The firmware was not the cause either: the
board answered its list.

Ruled out from the code and the e2e harness (a real `LpServer` over the
fake device): (a) Studio re-reads the list after the migration's reboot — a
new hello is a new window and `drive` syncs it; (b) the card keeps its
`DeviceId` and the record is keyed by MAC, so it survives the update; (d)
the store moves byte for byte and the board answers `bleEnabled: true`
after it. A 15-entry store with the same steps lists and leaves the switch
usable — only fullness breaks it.

**Fix** — `sync_access` fails only when the list itself cannot be read.
Past that, it returns the list as the board last answered it, with a
`refused` sentence beside it: a new key that cannot fit is not sent at all
(`This device's list is full (16 entries), so "<label>" could not be added.
Remove one to make room.`), and any other refusal ends the sync with the
list as it then stood. `AccessController` records the list either way and
shows the sentence as the panel's error (a later sync that goes through
clears it). No wire or firmware change.

**Regression coverage** —
`studio_device_e2e_tests::a_full_device_store_is_listed_and_its_switches_stay_usable_across_a_migration`
(a legacy C6 with a full store: listed with Bluetooth and anyone-nearby as
stored, the switch usable, the reason said; migrated; listed again) — times
out waiting for the list before the fix. `device_access_ops::tests::a_full_store_is_listed_and_the_add_that_cannot_fit_is_named`.

**Lesson** — A read-then-write conversation must hand back what it read when
the write is refused; "silent" error handling is only safe when the error
path cannot carry facts the success path would have kept. The panel also
conflated "not read yet" with "could not read" (`ble_enabled: None`), which
turned a refused add into a permanently locked switch.

Not settled from here: whether Yona's own browser showed the stale-record
variant (entries listed, switch locked) or the no-record one. A cached
record's `restartPending` (set when two listings disagree on Bluetooth,
cleared only by a restart Studio itself asked for and saw) also locks the
switch, and survives a reload; if the switch is still locked after this fix
on that browser, `localStorage['lp.access.device-lists.v1']` in the Studio
tab says which.
