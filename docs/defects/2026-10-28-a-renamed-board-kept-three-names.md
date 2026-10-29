---
status: fixed
found: 2026-10-28      # how: user report (Yona naming a flower test board after putting the PLAYFUL Choker Tryout example on it)
fixed: 2026-10-28
area: Studio board rename and push (`studio_controller.rs` push and rename paths, `lpa-devices` NameBoard), the wire (`SetDeviceName`), BLE advertising (`fw-esp32-radio/src/ble/advertising.rs`)
class: split-source-of-truth   # the board's name lived in the browser, the project's in the library, Bluetooth's in the push folder
related:
  - docs/adr/2026-10-28-one-board-name.md
  - docs/adr/2026-08-04-device-identity-anchored-in-silicon.md
---
# A renamed board kept three names

**Symptom** — Yona flashed a board, put the PLAYFUL Choker Tryout example
on it and renamed the board "Flower-Viatrix". The project kept the name
"PLAYFUL Choker Tryout", he could not find where to rename it, and a phone
scanning for the board still saw `LP-studio`.

**Cause** — three names, each with its own owner. The board's name was
Studio's alone (the registry, by the 2026-08-04 rule that it is never
written to the board). An example pushed to a board was copied into the
library under the example's own label. The firmware advertised the folder
of its first loaded project, and every Studio push lands in the folder
`studio`. Nothing joined them. The project's rename lived behind the
editor's Details menu.

**Fix** — one name (ADR 2026-10-28): a person's rename is written to the
board (`ClientRequest::SetDeviceName`, wire 46, into `/.lp/device.json`'s
`name`), reported in its hello and advertised as `LP-<name>` without a
reboot. A pushed example or library project lands as a copy named after
the board; a board rename renames the project only it plays, and back. The
editor's project panel title renames the project where it is read.

**Regression tests** — `lpa-devices/tests/board_name.rs`,
`lpa-server/tests/access_gate.rs`
(`naming_the_board_writes_its_identity_file_and_the_hello_says_it`),
`lpa-studio-core` `one_board_name_tests`.
