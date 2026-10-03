---
status: fixed
found: 2026-10-03      # how: report (a user asked for "leds on pin D5" on a XIAO C6)
fixed: this change
area: lpc-hardware boards/seeed/xiao-esp32-c6.json (+ .display.json); lp-cli hardware calibrate
class: absence-from-incomplete-search
related:
  - lp-core/lpc-hardware/boards/README.md
  - docs/defects/2026-09-23-xiao-c6-rf-switch-never-powered.md
---
# The XIAO ESP32-C6 profile marked D4/D5 "not-found", so no project could drive LEDs on them

**Symptom** — a user asked for LEDs on pin D5 of a Seeed XIAO ESP32-C6, and
LightPlayer could not do it. The board profile listed D4 and D5 as
`"status": "not-found"` with no GPIO, and its `gpio` list stopped at
`/gpio/21`. A board's WS281x endpoints are built from GPIO resources'
`display_label`, so `ws281x:local:D4` and `ws281x:local:D5` were not
endpoints on the board, in the emulated C6, or in Studio's pin list. The
board diagram showed both pads with a "not found" warning. The app agent's
board table listed `D0`–`D3`, `D6`–`D10` and nothing else.

**Root cause** — the profile's `not-found` came from the 2026-05-18
calibration run (`48b4db17d`, "polish calibration flow"). It was a result
of where the calibrator searched, not of the board. `lp-cli hardware
calibrate` only pulses the GPIOs the manifest already declares
(`calibration_manifest_update::gpio_candidates`), and the first profile
(`edbe337aa`) declared GPIO0–21 only. D4 and D5 are GPIO22 and GPIO23
(Seeed's XIAO ESP32-C6 pin list: "D4 | SDA | GPIO22", "D5 | SCL | GPIO23"),
so the search ran out of candidates and recorded the labels as not found.
When the profile moved from TOML to JSON (`3fa1e7288`), the README took the
XIAO's own entry as its example of `not-found`, described as "a silkscreen
label the variant does not actually expose", which made the result look
like a decision. Nothing in the repo's own C6 data disagreed:
`espressif/esp32-c6-devkitc-1.json` lists GPIO22/23 as plain GPIOs. On the
ESP32-C6 they are not strapping pins (those are GPIO4/5/8/9/15), not the
USB-Serial-JTAG pair (GPIO12/13, still reserved here), and not SPI flash
(GPIO24–30), and `fw-esp32c6`'s RMT driver accepts GPIO0–30.

**Fix** — `xiao-esp32-c6.json`: D4 → `/gpio/22` and D5 → `/gpio/23`, both
`assigned`, with the same "default I2C SDA/SCL" notes the XIAO S3 profile
uses. Also two `gpio` resources (`display_label` D4/D5, `gpio-output` and
`gpio-input`, aliases `IO22`/`GPIO22` and `IO23`/`GPIO23`, no
`deep-sleep-wake` because only LP GPIO0–7 can wake the C6). The display
sidecar gives both pads their GPIO and an `SDA`/`SCL` i2c chip in place of
the warning. The app agent's system-prompt snapshot was regenerated, and
now lists D4 (GPIO22) and D5 (GPIO23). `boards/README.md`'s `not-found`
example no longer names a real XIAO pad, and it now warns that a
calibrator `not-found` only covers the GPIOs the manifest listed.

**Regression coverage** —
`lpc_hardware::manifest::default_manifests::tests::default_esp32c6_manifest_resolves_d4_and_d5_to_gpio22_and_gpio23`.
It checks that both labels are assigned to GPIO22/23, that
`ws281x:local:D4`/`D5` are available endpoints on those addresses, and
that an LED output on D5 opens. It fails against the old profile. The
`lpa-boards` drift tests check that the sidecar agrees.

**Lesson** — a search that comes up empty only covers the candidates it
was given. "Not found" is a fact about the search, and it was written down
as a fact about the board. Calibration should run over every GPIO the chip
numbers, and any `not-found` should be checked against the vendor pin list
before it is trusted. When an example in docs is taken from real data, the
example becomes a claim about that data.
