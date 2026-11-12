# The first plain XIAO ESP32-S3 on the desk (XS1)

2026-10-28/29. Agent-run, with Yona at the desk.

- **Board:** XS1 `xiao-s3-1`, a plain Seeed XIAO ESP32-S3. This is
  LightPlayer's canonical S3; FS3 is the 16 MB Plus. ESP32-S3 (QFN56) rev v0.2,
  ESP32-S3R8: 8 MB octal PSRAM in the package (eFuse `PSRAM_CAP` 8M,
  AP_3v3). External flash is a GigaDevice GD25Q64, 8 MB (JEDEC `c8 40 17`),
  eFuse flash type 4 lines, SR2 `0x02` (QE set at the factory). 40 MHz
  crystal, MAC `44:BD:8D:76:CD:A0`. Antenna on its U.FL socket, USB straight
  into the Mac (not the switchable hub), loose with no fixture and no strips.
- **Registered** in the desk's `board` registry: mark XS1, slug `xiao-s3-1`,
  role `test`, `lp_board = "seeed/xiao-esp32-s3"`.
- **Factory firmware** (Seeed's Arduino demo, `bb76cb1`, IDF v5.4.1, 8 MB OTA
  table) was dumped whole before anything was written: two 8 MB reads,
  byte-identical, in `~/.photomancer/firmware-archive/seeed-xiao-esp32s3/`
  (not in git).

Every claim below comes from the board's own console over USB (`lp-cli link
capture`) unless it says otherwise.

## Verdicts

| check | result |
|---|---|
| Board profile | `seeed/xiao-esp32-s3` (#1328, merged). Stamped with `lp-cli hardware stamp`; release 2026.10.28-6 boots `hardware manifest: seeed/xiao-esp32-s3 (XIAO ESP32-S3)` |
| Install and hello | `lp-cli firmware install --release latest --target esp32s3-8mb`: written, read back, booted, proto 45, chip revision 0.2 |
| PSRAM | `psram: on, 8388608 bytes at 0x3c800000, self-test passed`; Rust's allocations go there first |
| Heap policy as on FS3 | internal free at boot 135,356 B (FS3 135,316); dram_seg arena 83,968 + dram2 73,744 + radio 49,152 |
| LCD_CAM | Skirt 620 five (5 × 124 on D9, D10, D5, D8, D7): `[lcd-ws281x] frames=2001 timeouts=0 lanes=0x1f`. No strips attached, so counters only |
| Two cores | `[core1] render helper on`; `jobs` climbing every window |
| Bluetooth | advertising `LP-cda0`, then the project's name. lightplayer.app (Mac Chrome, CDP central) connected over Bluetooth: card "Bluetooth · connected", firmware v2026.10.28-6, access "edit · no password" |
| X → Y update over USB | 2026.10.28-3 → -6 over lp-link channel 3 in 34.7 s: backup, core on trial, confirmed, engine (783 deflated chunks), project auto-loaded |
| Flash headroom on 8 MB | gate 2,983,335 B (64 KiB floor). `app.bin` 3,309,568 B of a 6 MiB factory partition; update headroom 3,997,696 B; core 29,520 B from its next 64 KiB page. The table ends at `0x790000`, leaving 448 KB slack |
| fps, Skirt 620 five | 56.2 fps on 2026.10.28-6 (DIO), p99 ≤ 25 ms; FS3 52.8 on the fps lane's head, also DIO. **66.6 fps in QIO** (this PR) |
| Wi‑Fi joins | 10/10 at 20 dBm, 10/10 at the 15 dBm cap, each on the first attempt (#1302) |
| Crystal | +7.2 ± 1.2 ppm against the Mac (FS3 +13.5 ppm, same method) |
| Radio frame-rate budget | joined and idle −4.7 % (ceiling 10 %), LAN link busy −9.7 % mean (ceiling 50 %), p99 ≤ 50 ms |

## The emulator first

Release 2026.10.28-6's merged image, padded to 8 MB, booted ROM-up in
`lp-emu-esp32s3` (`lp-emu` at `85f1f7a66`). Its console shows the same facts
as XS1's: PSRAM 8 MB with the self-test passed, the tree store formatted, the
core 1 helper on, 8 LCD lanes declared, proto 45, Bluetooth advertising. One
difference: **the emulated flash part is Winbond `ef 40 17`, while XS1's is
GigaDevice `c8 40 17`.** Both read DIO on the release, and only XS1's part is
on the S3 QIO list after this PR. Whether the emulated part should follow the
board profile is a follow-up.

## Flash read mode: QIO on the GD25Q64

The S3's QIO list was empty, so every S3 read its flash in DIO, FS3 included
(`85 20 18`, Puya, SR2 `0x02`). XS1's part is the first read on an S3.

The switch's first run on S3 silicon was refused and put back, safely:
`the controller did not read back as set`. A diagnostic build read the
registers after the ROM's `SPIMasterReadModeCnfig(QIO)`:

| | DIO (before) | QIO (after) |
|---|---|---|
| SPI0 `CTRL` | `00ac2008` (FREAD_DIO) | `012c2008` (FREAD_QIO) |
| SPI1 `CTRL` | | `012ca008` (FREAD_QIO) |
| SPI0 `USER2` | `0xbb` | `0xeb` |
| SPI0 `USER1` | `5c000003` | `5c000005` |
| 32 bytes at `0x0`, DIO vs QIO | | identical |

Every check passed but one: the address length. The C6's ROM sends the
0xEB read's mode byte as the low 8 bits of a 32-bit address phase
(`USR_ADDR_BITLEN` 31). The S3's keeps 24 address bits (23) and covers the
mode byte in the two extra dummy cycles. Both are correct ways to issue
0xEB. `Mspi::qio_verdict_with` now takes the chip's address length; the C6
still calls `qio_verdict` with 31.

With it, on XS1:

| | DIO (2026.10.28-6) | QIO (this PR, same base `85f1f7a66`) |
|---|---:|---:|
| Skirt 620 five | 56.2 fps | **66.6 fps** (+19 %) |
| engine digest check (2.1 MB through the cache) | 159 ms | 103 ms |
| core hash at boot, project rendering | 152 ms | 110 ms |

What it costs: the QIO switch is a RAM function, reachable now, so `.rwtext`
grows. The S3's static RAM moves 400 B (the arena and radio region start
`0x190` higher), and the main stack total drops 58,916 → 58,516 B
(`lp-emu:esp32s3:t1`). The core grows 752 B, leaving it 28.8 KB from its next
64 KiB page. The C6 calls the same verdict with its own address length, and
its figures do not move.

Soak: 20 reboots, each `[flash] 80 MHz, set before the core QIO (jedec
1740c8, sr2 02)`, and each hello's core SHA-256 equal to the built
`core.bin`. A project upload (tree-store writes under QIO), then a reboot:
the project loaded and rendered at 66.6 fps. No status register was
written. FS3's Puya part stays off the list until a sitting with FS3 soaks it.

**FS3's Puya part, the same day.** FS3 (XIAO ESP32-S3 Plus, 16 MB, `85 20 18`,
SR2 `0x02`, QE set at the factory) on the shipped image from main
`3dc8a3527`, with its part added to the list: `[flash] 80 MHz, set before the
core QIO (jedec 182085, sr2 02)` on 20 of 20 reboots, each hello's core
SHA-256 equal to the built `core.bin`, the startup project loaded every time.
A project upload written under QIO (the 2,048-LED `Core1 8x256`), then a
reboot: it loaded and rendered. Skirt 620 five 55.6 → **65.8 fps**; core hash
at boot 160 → 116 ms. No status register was written.
Not run: an over-the-air update written by a QIO core.

## Wi‑Fi

Image: PR #1302 merged with main (`ae8e52352`), `wifi` on, BLE advertising,
Skirt 620 five rendering, `lp-bench-ap` at −34 dBm. Each trial is a reboot;
it counts as a join when an address arrives within 50 s.

| image | joins | first attempt | associated in | address after boot |
|---|---:|---:|---|---|
| 15 dBm cap (#1302) | 10/10 | 10/10 | 713–828 ms | 1.6–6.9 s |
| 20 dBm (cap removed, uncommitted) | 10/10 | 10/10 | ~730 ms | 1.8–7.2 s |

So FS3's 20 dBm failure is that board's (one Plus unit, revision unknown),
not the S3's. #1302 keeps the cap for every S3, which costs about 5 dB,
roughly 40 % of free-space range. Restoring full power where it works is
queued.

Frame-rate cost, same image, means of 5 s windows:

| state | fps | p99 | longest frame |
|---|---:|---:|---:|
| Wi‑Fi off | 54.8 | ≤ 25 ms | 26 ms |
| joined, idle, 2 min | 52.2 (−4.7 %) | ≤ 50 ms | 617 ms |
| `lp-cli link rtt lan:` | 49.5, worst window 41.0 (−9.7 % mean) | ≤ 50 ms | 130 ms |

LAN request RTT p50 89 ms, p99 289 ms; 21.5 KiB/s board → host.

## Findings beside the walk

- `lp-cli firmware install` onto a board that came from somebody else's
  firmware writes and verifies, then exits with an error: `the board came
  back with its filesystem Formatted, not mounted — its files are in the
  backup`. There was no backup and nothing to restore; a fresh store is
  what such a board should have.
- Studio before #1328 showed the stamped board as the raw id
  `seeed/xiao-esp32-s3`. With #1328, an 8 MB S3 on native USB matches this
  board and the S3 DevKitC-1, so install asks "Which board is this?".
- The firmware's compiled-in S3 default is still the Plus (same pin map), so
  an unstamped plain board says Plus in its hello.
- The user LED (GPIO21 per Seeed) and the back pads are not in the profile
  yet; nothing on the desk has checked them.
