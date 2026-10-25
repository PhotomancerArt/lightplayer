---
status: open
found: 2026-10-22      # how: hardware-walk (the S3 radio plan's M4 desk walk, sittings 1 and 2, FS3)
area: lp-fw/fw-esp32s3 `wifi` image × esp-radio's station on the S3 (lp-fw/fw-esp32-radio/src/net/esp_station.rs)
class: untested-path   # the S3's station had only ever scanned on silicon (M1's probe) and joined in the emulator, where the network seam stands in for the radio
related:
  - lp2025/2026-10-20-2335-s3-radio-and-ota (m4-s3-wifi.md, AC5–AC7)
  - docs/adr/2026-10-21-s3-radio-memory.md
  - docs/defects/2026-10-22-the-s3-truncates-ws281x-frames-at-620-px-with-bluetooth-on.md
---
# The S3's Wi‑Fi image never authenticates to an access point

**Symptom** — FS3 (`fixture-s3`, XIAO ESP32-S3 Plus, 16 MB,
D8:3B:DA:47:29:70, antenna attached) on the `wifi` image (#1253) scans
fine — 29 networks, the desk's `lp-bench-ap` at −35 dBm — but every join
ends the same way:

```text
[wifi] trying lp-bench-ap
[wifi] attempt ended: authentication expired (ended)
[wifi] failed lp-bench-ap · frames in 0 out 0
```

Sitting 1 (2026-10-22): 5 of 5 attempts to `lp-bench-ap`, and the house
network before it the same way. A desk C6 (`lp-5490`) is joined to
`lp-bench-ap` at −38 dBm with the same saved password, so the network and the
password are good. "Authentication expired" is the 802.11 authentication
exchange timing out (the IDF's reason 2), before the WPA handshake starts:
the board is not heard, or does not hear the answer, at that step.

**Ruled out** (sitting 2, the same image with one change each, 60 s from a
reboot):

| image | what changed | attempts | authentication expired | joined |
|---|---|---:|---:|---:|
| A | the Wi‑Fi blobs' IRAM classes back in IRAM (`ESP_HAL_CONFIG_PLACE_WIFI_IRAM_IN_FLASH=false`, `…_RX_SLP_…=false`; arena 112 KB so the stack keeps 46 KB) | 5 | 4 | 0 |
| B | Bluetooth never started (no BTDM controller, no coexistence traffic) | 3 | 2 | 0 |

So it is neither flash-cache timing (M4's IRAM→flash switches) nor
Bluetooth coexistence. Neither experiment image is committed.

**Also seen, not this defect** —
- The `wifi` image's radio heap peaked at 48,038 of 49,152 B during the
  failed joins (0 B overflowed to main).
- FS3's startup project, the 620-pixel "Skirt 620", reset the `wifi` image
  while it loaded; the recovery rule then skipped it. The `wifi` image has far
  less heap than the Bluetooth image that runs it. That pressure is M10's
  (PSRAM), not this defect's.

**Root cause** — not established. Candidates, untested:

- Wi‑Fi transmit on this chip at the authentication step: the PHY's
  calibration or TX power on the S3 (scans can hear without the board being
  heard), or the S3 blob's handling of the station's management frames.
- The station's configuration as the shared crate builds it for the C6
  (`fw_esp32_radio::hardware::espnow_controller_config`, the station's
  auth-method and PMF settings): correct on the C6, never exercised on the S3
  before.
- M4's `NetChip` facts for the S3 (lp-net pinned to core 0, 12 KB stack):
  the join runs on that thread.

**What it blocks** — M4's desk walk (AC5 join and LAN link, AC6 frame-rate
cost, AC7 warm resets, LED output under a scan). `wifi` is off by default, so
no shipped image changes.

**2026-10-22, the emulated walk (S3 radio M5, #1273)** — the same `wifi`
image joins in the emulator. Two emulated S3s (`lp-emu:esp32s3:t1+net=lan`)
each saved a network over USB, joined it, took DHCP leases, answered mDNS and
DNS‑SD, and served Studio's secure LAN link. Studio then edited one of them
over that link (docs/reports/2026-10-22-s3-wifi-emulator-walk.md). On that
arm the network seam stands in for esp-radio's station (`net_bringup` picks
`SeamStation`, not `EspStation`). So the walk exercises the station task's
join policy, the IP stack, the LAN endpoint and the `lp-net` thread on its S3
facts (core 0, 12 KB stack). Those work, which makes candidate 3 unlikely.
It says nothing about candidates 1 and 2: the radio's transmit, and the
station configuration `EspStation` hands esp-radio, are exactly what the
seam replaces. The silicon failure is therefore at or below `EspStation`:
the configuration, the blob or the radio.
