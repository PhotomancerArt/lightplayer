---
status: fixed
found: 2026-10-03      # hardware-walk rehearsal (C6 repartition G1, scene 4, the spare XIAO C6)
fixed: see the commit that adds this file
area: fw-esp32c6 BLE start × lpa-server access_store × lpa-studio-core AccessController
class: stand-in-divergence
related:
  - lp2025/2026-10-01-1843-c6-repartition (PR #896, G1)
  - docs/adr/2026-09-23-ble-access-model.md
  - docs/adr/2026-09-24-easy-bluetooth-access.md
  - docs/adr/2026-10-02-c6-repartition-and-layout-migration.md
---
# A held board runs a fresh access store in RAM: Bluetooth on, "Who has access 1"

**Symptom** — the G1 rehearsal's bypassed flash (scene 4: the branch's image
written over a board still on the old layout). The board booted held
(`[FS] Mount failed (filesystem corrupt), holding: not formatting`), on a
memory filesystem, and then: `[ble] enabled (device store, or none: on by
default) — starting`, `[ble] advertising as LP-8e30`. Studio's card read
"Who has access **1**". The board's real `/.lp/access.json` — 16 entries — was
still in the old region at `0x310000`, untouched; after Finish update the
card read 17.

**Root cause** — the access store is read off the board's filesystem, and a
held board's filesystem is a RAM stand-in that has no store. Two readers took
"no store" at face value:

- **The firmware.** `read_device_store` on the RAM fs found no file, which the
  easy-access amendment defines as a *fresh* board: `DeviceAccessFile::fresh()`,
  Bluetooth on, locked, no keys. That rule is for a board that never had a
  store. A held board has one; it is unreachable.
- **Studio.** The USB connect's sync ("physical connection is access") read
  the empty list and added this browser's key to it (a toast, with Undo),
  and the card listed that one entry as the board's access.

**What a held board granted, before the fix** (read from the code: the
firmware's BLE start, `lpa-server` `access_store`/`access_state`, the
classifier, and Studio's `AccessController`; the rehearsal's console and card
agree):

| Link | Before Studio connects over USB | After a USB connect |
|---|---|---|
| USB (trusted) | edit, no login — as on every board (the model ADR: physical possession is the recovery path) | edit |
| Bluetooth | radio **on** and advertising; `Hello` and `Login*` only; no installed secret (RAM store empty, no project loaded, so no sidecar), so no login can succeed: **nothing** | this browser's key (and, signed in, the account key and account passwords) **in RAM** → those holders unlock at **edit**; and a USB user's switches (`open`, `bleEnabled`) land in RAM too, so `open` would let anyone in range at **play** until the next reboot |

Every Studio write landed in the memory filesystem: **nothing was written to
flash**, the real store was never touched, and Finish update moved it byte
for byte (W9; the rehearsal's 16 → 17 is the ordinary add after the move).
So nothing was lost.

**Was it more open than the board's own list?** Yes, in one case the access
ADRs decide against. If the board's real store said `bleEnabled: false` (the
owner turned Bluetooth off), or was damaged (`locked()`, Bluetooth off), the
held board advertised anyway and, after any USB connect, let the plugging-in
browser's holders unlock over the air at edit — and could be switched `open`.
The model ADR's rule is that an unreadable store means locked ("damage only
ever takes access away"); the easy-access amendment carved out only a
*missing* store as fresh. A held store is not missing. Where the real list
had Bluetooth on, the held board was no more open than it: the keys Studio
adds over USB are the ones the same connect adds to a mounted board.

**Fix** — the held state fails closed, inside what the ADRs already decide
(no access-model change, no persisted-format change, no wire change):

- `lpa_server::access_store::device_store_at_boot(fs, fs_boot_state)` is
  `DeviceAccessFile::locked()` on `FsBootState::LegacyHeld` (else
  `read_device_store`), and the C6 firmware decides Bluetooth from it:
  `[ble] off (files held for the layout change: the device store waits with
  them)`. No untrusted link exists on a held board; USB keeps edit, as on
  every board. The next boot after Finish update reads the real store.
- Studio does not sync access with a held board (`AccessController`:
  `holds_its_files`, from the hello's `fs`) and shows no "Who has access" for
  it: nothing is added to a RAM store, and the card does not pass a RAM list
  off as the board's.

**After the fix, a held board grants:** USB — edit (unchanged; a hand-built
client could still write the RAM store over it, which reaches nothing: no
radio link exists and the store is gone at the next boot). Bluetooth —
nothing: the radio stays off until Finish update.

**Not changed (same class, outside this fix):** the other memory-FS boots —
no `lpfs` row in the flashed table (`FsBootState::Memory`, new with this
plan's runtime table lookup) and a flash filesystem that fails to initialise
(`[WARN] Flash FS failed … falling back to memory`, on main too) — still read
"no store" as fresh, Bluetooth on. A board there may also have a real store
it cannot reach. Whether those should boot locked is a question for Yona, not
decided here; they are reached only by a mis-flash or a failing flash part.

**Regression coverage** — `lpa-server`
`access_store::tests::a_held_board_boots_locked_with_bluetooth_off` (before:
the firmware's `read_device_store` on the held RAM fs answers `fresh()`,
Bluetooth on — the test's own premise line); `lpa-studio-core`
`studio_device_e2e_tests::a_held_board_gets_no_access_entries_and_shows_no_access_list`
(the fake board held after a pull mid-update; before the fix it failed with a
second "added" toast, generation 1 → 2, and a panel listing the one RAM
entry); and `just walk-migration-emu W9`'s new steps "the held board kept
Bluetooth off" and "the card shows no access list for it". Not walked on
silicon.

**Lesson** — "missing" and "unreachable" are different facts about a
persisted file, and a stand-in filesystem makes every file look missing. A
reader whose default for "missing" is permissive needs to know which one it
has.
