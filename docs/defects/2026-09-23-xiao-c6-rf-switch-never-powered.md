---
status: fixed
found: 2026-09-23      # how: hardware-walk (the BLE spike, spikes/ble-lab)
fixed: db129b4cf
area: fw-esp32c6 board init (board/esp32c6/init.rs) — no board-specific radio setup
class: assumed-context
related:
  - lp2025/2026-09-23-1428-ble-remote-control/
  - docs/adr/2026-07-28-esp32c6-flash-budget.md
---
# fw-esp32c6 never powers the XIAO ESP32C6's RF switch, so the radio runs into an unpowered antenna switch

**Symptom** — during the BLE spike (fw-esp32c6 `test_ble`, XIAO ESP32C6
`A0:F2:62:87:B4:8C`, on-board ceramic antenna, about 0.5 m from the laptop on
a desk with other equipment), a Web Bluetooth link from Brave dropped **7
times in about 84 s**. Every drop was HCI reason `0x08`, connection
supervision timeout, meaning the two radios stopped hearing each other.
Uptime between drops was 0.8 s to 39 s, and drops came faster under traffic.
After driving GPIO3 LOW and GPIO14 LOW at boot, the same test battery ran
with **0 drops**: 20 echoes, two 200-packet bursts, two 200-write runs and
30 s idle.

**Root cause** — the XIAO ESP32C6 puts an FM8625H RF switch between the
chip and its two antennas: the on-board ceramic one and a U.FL connector.
Seeed's docs say GPIO3 must be driven LOW to power the switch, and GPIO14
then selects the antenna (LOW = ceramic, the default; HIGH = U.FL). ESPHome
changed its XIAO C6 support to initialize these pins at boot "to overcome
radio issues" (esphome/esphome#19406). Nothing in `fw-esp32c6` drives
either pin. The firmware has no board-specific radio init at all, because
it presumes the chip's RF output reaches an antenna with no help, which is
true of a bare module and false on this board.

**Scope — what this likely touches beyond BLE** — every radio use of every
XIAO C6 we ship: the ESP-NOW group-sync path today (the `radio` feature),
and WiFi later. The PLAYFUL choker is a XIAO C6. None of this has been
measured on ESP-NOW yet: the evidence is **one A/B on one board, over BLE**.
Treat the ESP-NOW claim as a strong hypothesis, not a finding.

**Fix** — a compiled-in board-quirk table, keyed on the board id of the
hardware manifest **in effect** (`HwManifest::board_id()`, the compiled-in
fallback included — and the C6's fallback *is* `seeed/xiao-esp32-c6`). The
pure table is `lpc_hardware::board_quirks_for`
(`lp-core/lpc-hardware/src/manifest/board_quirk.rs`); `fw-esp32c6` applies it
in `board/esp32c6/board_quirks.rs`, called from `boot_firmware` right after
the manifest loads and before the ESP-NOW radio init. For the XIAO it drives
GPIO3 LOW and GPIO14 LOW, holds both for the life of the program, and logs
`Board quirk applied: xiao-c6-rf-switch (GPIO3=LOW GPIO14=LOW)`. Any other
board id is left alone. `hardware.json` and its schema are unchanged: a new
field there would be a persisted-format change. Choosing the U.FL antenna
(GPIO14 HIGH) is future work, as a user setting, not a quirk.

**Regression coverage** — `lpc-hardware`'s `manifest::board_quirk::tests`
pin the table: the XIAO id and the C6 fallback manifest take the quirk; every
other compiled-in board, every other checked-in board file (the C6
DevKitC-1 among them) and a set of near-miss ids take none. The emulated C6
boots the shipped image under `--strict-bus` with the quirk applied and no
fault. The *effect* has no automated coverage: the emulator has no RF model.
The desk check is `spikes/ble-lab` with the drop counter (the BLE plan's M2),
which a phone or laptop at a fixed distance can repeat.

**Lesson** — a dev board is a chip plus decisions the board maker made, and
some of those decisions need firmware cooperation to work at all. A radio
that "works" at 0.5 m can still be running with its antenna path unpowered;
the only visible symptom was a flaky link.
