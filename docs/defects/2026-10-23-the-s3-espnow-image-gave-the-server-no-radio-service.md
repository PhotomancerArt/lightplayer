---
status: fixed
found: 2026-10-23      # how: hardware-walk (FS3, the PSRAM-default check, #1277)
area: lp-fw/fw-esp32s3 `espnow` image × lpa-server's hardware facts (lp-fw/fw-esp32s3/src/main.rs)
class: untested-path   # the S3's espnow tier was built and linted, never asked by a host whether it had a radio
related:
  - lp2025/2026-10-20-2335-s3-radio-and-ota (M10 follow-up, PSRAM and ESP-NOW on by default)
  - docs/adr/2026-10-21-s3-radio-memory.md
---
# The S3's ESP-NOW image gave the server no radio service

**Symptom** — FS3 (XIAO ESP32-S3 Plus, 16 MB), antenna attached, on the
split image with `espnow` in the default (#1277): the boot said

```text
[fw-esp32s3] ESP-NOW radio ready: device_id=RadioDeviceId(1881753562) channel=11
```

and the hello said `"radio":false` with no `svc.radio-espnow` in its
features, while the image's manifest (`manifest-core.expected.json`) listed
`svc.radio-espnow`.

**Cause** — `fw-esp32s3`'s `main.rs` registered the ESP-NOW driver with the
hardware system (`add_radio_driver`) and then passed `None` as the server's
radio service (`LpServer::new_with_hardware_services`). The server derives
`HardwareFacts.radio`, and with it `svc.radio-espnow`, from that argument,
and a project's Radio node opens its radio through it. So on every `espnow`
image since S3 radio M4 the radio was up and nothing could use it, and the
hello and the manifest disagreed. Nothing caught it: the emulator gates read
neither field on the S3, and `espnow` was off by default until this change.

**Fix** — under `s3_espnow` the hardware system is the server's radio
service, as on the C6 (`fw-esp32c6/src/main.rs`); a radio-free or
Bluetooth-only image still passes `None`.

**Regression coverage** — `lp-cli/tests/emu_s3_link_gates.rs`'s ledger
gate asserts the shipped image's hello says `"radio":true` and
`svc.radio-espnow` (the S3 gate, `just test-emu-esp32s3-gate`); and the FS3
check's own hello.
