# The S3's PSRAM, walked on silicon (M10 P6)

- **Board:** FS3 (XIAO ESP32-S3 Plus, 16 MB), antenna attached;
  D8:3B:DA:47:29:70. Leased, two sittings of 16 and 15 minutes,
  2026-10-22 17:36–18:09 PDT, the known-good release image (`lp-cli firmware
  install --release latest`, 2026.10.22-9/-11) flashed first and last.
- **Images:** `fw-esp32s3 --features psram` at `270dd9fd7` plus the `[led]`
  counter line (committed as the next commit), plain (not split), one per
  heap policy (`LP_PSRAM_HEAP`), PSRAM at 80 MHz. The baseline is main's own
  shipped image (2026.10.22-11, `d3cf29ca7`: M11's 80 MHz flash merged), split,
  as the release ships it.
- **Load:** `projects/test/skirt-620`, 620 LEDs on **two** outputs (2 × 310),
  shader compiled on the board; not the PSRAM spike's 4 × 155.
- **Bluetooth: connected** in every row: Mac Chrome via CDP as the central
  (`spikes/ble-lab`'s pipe page, `lp-cli link capture blepipe:`), board ~1 m
  away, link open with heartbeats flowing, no editing traffic. **Mac Chrome is
  not a phone**; no row here is a Bluefy number. Each row's capture shows the
  board granting the connection's parameters after the reboot
  (`[ble] link1: granted …`).
- **Method:** a USB capture (`lp-cli link capture <port> --request reboot
  --seconds 75`) per row. fps is the frame counter over each `[perf]` 5 s
  window, median of the 11 windows after the load; compile is the shader
  node's own `elapsed=`; internal free and largest block are the heartbeat's
  (internal RAM on a `psram` image); PSRAM used is the `[PSRAM]` ledger line;
  LED trips are the RMT driver's `guard_trips` over frames, both outputs,
  cumulative to 1,500 frames each (`[led]`).

## The cost table

| | main (no PSRAM) | `off` (PSRAM up, heap internal) | **`all`** | `split:512` | `split:1024` |
|---|---:|---:|---:|---:|---:|
| fps (median) | **22.15** | 22.19 / 22.15 | **21.51 / 21.40** | 22.06 | 22.00 |
| fps against main | — | +0.2 % / 0.0 % | **−2.9 % / −3.4 %** | −0.4 % | −0.7 % |
| render tick (median) | 40 ms | 44 / 44 ms | 44 / 46 ms | 42 ms | 45 ms |
| frame p99 (worst window) | ≤ 75 ms | ≤ 75 ms | ≤ 75 ms | ≤ 75 ms | ≤ 75 ms |
| frame max (whole run) | 198 ms | 181 ms | 189 ms | 184 ms | 183 ms |
| shader compile | 98 ms | 99 / 99 ms | **105 / 105 ms** | 101 ms | 100 ms |
| internal heap free, running | **50,780 B** | 47,724 / 47,692 B | **236,080 B** | 164,164 B | 150,892 B |
| largest internal block | 39,584 B | 36,352 / 35,864 B | **149,928 B** | 74,704 B | 73,736 B |
| Rust bytes in PSRAM | — | 0 | **137,884 B** | 76,720 B | 64,892 B |
| LED guard trips | not counted | 24.8 % / 24.0 % | 24.2 % / 21.7 % | 27.9 % | 25.1 % |

Two runs where a value has two. Read it this way:

- **`all` frees 185 KB of internal RAM (50.8 → 236 KB free, largest block
  39.6 → 150 KB) for −3.2 % fps and +7 % compile.** That is the spike's trade
  (−3.6 %, +5 %) on main's own 80 MHz flash.
- **`split:512` costs almost nothing (−0.4 %) and frees 113 KB** of internal
  free (to 164 KB), but moves only 76.7 KB of Rust bytes: the rest of its gain
  is the fragmentation PSRAM takes with it. M10's ship rule ("≥ 100 KB moved
  at ≤ 1 % fps") is not met on the bytes moved, and is met on internal free.
  That is the ruling's question.
- **PSRAM up and unused (`off`) costs nothing** against main: the 3 KiB the
  arena gives back for the init's IRAM shows as 3 KB less free.
- **LED truncation is not PSRAM's.** Every variant, `off` included, truncates
  a fifth to a quarter of frames with Bluetooth **connected** on 310-pixel
  channels. In the first sitting two rows ran with Bluetooth only
  advertising (the page had not reconnected) and truncated 4.8–5.1 %. So
  connected Bluetooth multiplies the open defect's rate
  (`docs/defects/2026-10-22-the-s3-truncates-ws281x-frames-at-620-px-with-bluetooth-on.md`,
  which this walk adds a section to). main's image carries no counter, so
  main's own rate is not in the table.

## The other rows

- **The clocks, on silicon:** every `psram` boot printed `[flash] 80 MHz, set
  before the core …` (M11's switch found the flash at 80 and left it) and
  `[INIT] mspi clocks: core 160 MHz, flash 80 MHz (SPI0) 80 MHz (SPI1), psram
  80 MHz`.
- **20 warm resets with PSRAM holding data** (`all`, a project running, 137 KB
  of heap in PSRAM): 10 requested reboots and 10 `espflash reset`s
  (RTS/DTR through the USB-Serial-JTAG), alternating. **20/20** booted with
  `self-test passed` and served frames after; no bootloader crash.
- **A tree-store save while rendering** (`all`): a full project deploy
  (`lp-cli upload`, a tree-store batch) while the 620-pixel project rendered;
  the board kept rendering at 20–21 fps through and after it (uptime
  continuous, no restart, no panic), and the PSRAM heap read back the same
  figures after it. The LED frames during the save are inside the defect's
  noise above; the decoded-frame check needs M9's LCD_CAM or an RX rig.
- **JIT code in PSRAM, the publish rule** (`fw-esp32s3 --features
  test_psram`, PSRAM at 40 MHz): code rewritten in place ran **stale before
  the publish** (`f(5) = 22`) and fresh after it (`16`); patterns over all
  8 MiB, 0 wrong words, twice. The emulator never shows the stale half, by
  design.

## Owed

- **First light.** The S3 prints no `[perf] first light` line (the C6's
  `first_light.rs` has no S3 twin), and the heartbeat's uptime cannot stand in
  for one. A first-light line for the S3 is its own small change.
- **Project load time** (the spike's ledger bracket): the product image's logs
  carry no timestamps over the load; not measured.
- **Wi‑Fi joined** with traffic: waits for M4 (#1253).
- **Power** (no USB meter on FS3's hub port) and **the plain XIAO ESP32-S3**
  (not on the desk).
- **An editing load over Bluetooth** (Studio pushing changes): this walk's
  connection carried heartbeats only.
