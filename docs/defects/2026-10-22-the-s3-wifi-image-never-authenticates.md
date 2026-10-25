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

**2026-10-25, the cause: too much transmit power (FS3 sitting, s3-wifi-push)** —
On current main (PSRAM the heap, Skirt 620 five loaded and rendering), the
`wifi` image still never joined: 0 of 13 at ~1 m (−38…−44 dBm), 0 of 13
with FS3 20–30 cm from the access point (−13…−18 dBm). The blob's TX power
ceiling read back as 80 quarter dBm, so it was already transmitting at
20 dBm. With `WifiController::set_max_tx_power(60)` (15 dBm) before the
join, FS3 joined **10 of 10, each on the first attempt, in 714–828 ms**,
close range, project rendering; the earlier 8.5 dBm sitting (E) failed at
~1 m. The failures at 20 dBm were "disassociated due to inactivity" and
"authentication expired", with the AP's BSSID heard every time: the board
reached the AP and its frames were not understood. Distance never mattered.

What the board is: Seeed's XIAO ESP32-S3 Plus schematic
(`XIAO_ESP32S3_Plus_V1.1_SCH_260115.pdf`, files.seeedstudio.com) has **no
RF switch and no antenna GPIO** (U.FL → 1.2 pF / 3.3 nH / 1.8 pF → LNA_IN;
nothing for firmware to drive, unlike the XIAO C6's GPIO3/GPIO14). Its
revision table says V1.1 (2025-07-24) changed "R5 0R → L5 24nH, C7, C8
10pF → 12pF": a series inductor on XTAL_P and the 40 MHz crystal's load
capacitors. Measured from the heartbeat's `uptime_ms` against the host
clock over ~10 min, FS3's crystal runs **+13.5 ppm** against the Mac while
two desk C6s read −6.8 and −10.8 ppm — FS3 is ~22 ppm fast of them, the
direction a too-small load capacitance pulls it. FS3's revision is not yet
known. A V1.0 board's crystal (and its unfiltered XTAL_P harmonics) is the
likely reason its 20 dBm signal is unusable where a cleaner, lower-power one
is fine; the plain XIAO S3 arriving next is the check on whether other S3
boards need the cap.

**Fix**: the S3's `NetChip` caps the station's TX power at 15 dBm
(`fw-esp32s3/src/net/net_chip.rs`, `MAX_TX_POWER_QDBM`), applied before
every join; the C6 keeps the blob's 20 dBm. The station's log now names more
disconnect reasons and says whether the AP was heard.
