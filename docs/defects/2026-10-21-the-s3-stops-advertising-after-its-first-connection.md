---
status: fixed
found: 2026-10-21      # how: the S3 radio plan's M3 desk walk, FS3 with Mac Chrome as the central
fixed: 2026-10-21
area: lp-fw/fw-esp32s3/src/ble/ble_controller_config.rs (the BTDM controller's `max_connections`)
class: config-masked-defect
related:
  - docs/adr/2026-10-21-s3-radio-memory.md
  - docs/adr/2026-09-24-ble-transport.md
---
# The S3 stopped advertising after its first Bluetooth connection

**Symptom** — FS3 (XIAO ESP32-S3 Plus, the `ble` image, #1251) advertised and
took one central (Mac Chrome through Studio). From that moment the board's
console logged `[ble] advertising failed; retrying in 1 s` once a second for
as long as the central stayed connected: the second link slot was never
offered, and the retry loop ran all the while.

**Cause** — `esp_radio::ble::Config::max_connections` means two different
things on the two controller families. On the C6's NPL controller it is the
connection count. On the S3's BTDM controller esp-radio passes it as
`ble_max_act` (`third_party/esp-radio/src/ble/os_adapter_esp32c3_s3.rs`), the
controller's **activities**: every advertising set and every connection
together. The S3's config set it to the link slot count (2), copying the C6's
rule; with one connection open the controller refused the advertising set
that would offer the second slot. The emulator could not show it: no central
ever connects to an emulated board.

**Fix** — `max_connections` on the S3 is the slots plus one for the
advertiser (3; esp-radio's default is 6). The config's doc names what the
number means on BTDM.

**Lesson** — a shared crate's injected chip fact keeps the shape of the
first chip's API; the same field name on a second controller family needs its
meaning checked against that family's adapter, not carried over.
