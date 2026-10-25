---
status: paying-down
since: 2026-10-21      # Bluetooth on by default in the S3 image (#1254), the S3 machine interpreted
logged: 2026-10-22
area: lp-emu/esp/lp-emu-esp32s3 (the Xtensa interpreter, lp-xt-emu) × Studio's wall-clock walks
related:
  - docs/defects/2026-10-22-the-emulated-s3-runs-a-rewritten-shader-from-its-stale-cached-blocks.md
  - docs/adr/2026-09-11-emulator-wasm-translator.md
  - lp-emu/lp-emu-jit/README.md
  - lp2025/2026-10-20-2335-s3-radio-and-ota (M7 P12a, #1263)
---
# The emulated S3 runs about forty times slower than real time

**Shape** — the ESP32-S3 machine is interpreted, both cores, with no fast
path. The Xtensa translator exists behind `--features jit`, but it still
escapes every instruction back to the interpreter, which makes it slower,
not faster. With Bluetooth on (the shipped default since #1254) and a
project rendering, the emulated S3 retires about 4.7 M instructions per
wall second: **1 s emulated takes about 42 s wall**, release or debug
`lp-cli` alike. The cost comes from the radio's tasks and a rendering
engine, which never leave the cores idle long enough to skip ahead. A board
with no project and no radio sleeps in `wfi`, which the machine skips, so it
runs several times faster.

> **Corrected 2026-10-22 (#1270).** The machine was *not* without a fast
> path: the shared Xtensa block cache was on. What made it slow was that the
> whole of SRAM1 is one executable region on this chip (DD81), so every
> stack push and heap write reached the cache as a range invalidation — a
> walk of its 2^18-entry table, **81 % of host time** (samply). The bus now
> records only stores that touch cached code, at every alias door; see
> "Paydown" below. The 42 s figure is the before.

**Carrying cost** — every walk that drives Studio in wall time stalls on the
S3:
- **Identify:** Studio's identify settles after 5 s of wall time; the S3's
  hello came about 180 s into the step.
- **Timers:** the BLE login drop is 10 s and the keepalives are about 1 s,
  both wall clock.
- **Updates:** an over-the-air X → Y with a rendering project takes tens of
  minutes of wall time.

`walk-ota-emu --s3` (#1263) reaches the board and the card offers Update,
but before the re-identify fix (M7 P12a's follow-up) it could not get past
identify. Every S3 emulator suite pays the same factor in CI minutes.

**Workarounds**
- Drive the S3 from `lp-cli` (`emu run --chip esp32s3 --host-link`), whose
  host is paced by emulated time, not wall time: P8's X → Y ran to
  `UpToDate` that way.
- Keep emulated S3 boards idle where the check allows: no project, Bluetooth
  off through the device store.
- Give wall-clock waits room (`--wall-timeout`, `WALK_UPDATE_MS`).
- Never quote an S3 wall time as a board's.

**Exit criteria** — the S3 within a few times real time with Bluetooth and a
project: a translated Xtensa core that keeps a stay's instructions
translated (the C6's translator ladder, `bench-emu-esp32v3`'s oracle rows),
or a measured idle skip for the radio tasks' polling. Measure it with the
same run: `lp-cli emu run --chip esp32s3 --host-link --rom-up-flash <x.bin>
--timeout 1s`, whose wall time is this entry's 42 s.

**Paydown (2026-10-22, #1270)** — `just bench-emu-esp32s3`, CI's shipped
image (`merged.bin` 8aff8eb67c33, Bluetooth on), user seconds, load ≈ 9,
main against the branch:

| row | main | branch | |
|---|---:|---:|---:|
| render-basic, steady (1 → 4 s emulated) | 55.0 s per emulated s (0.018×) | 3.41 s (0.293×) | 16.1× |
| render-620, steady | 53.9 s (0.019×) | 3.53 s (0.283×) | 15.3× |
| boot-idle, 1 s | 13.38 s (0.075×) | 1.58 s (0.633×) | 8.5× |

Every byte, count and frame identical (`scripts/emu/s3-oracle.sh`: cache
against `--no-block-cache`, and main against the branch, on boot-idle,
radio-boot, render-basic and render-620). What is left is interpreter
throughput — ~65 MIPS against the ~220 M instructions an emulated second of
rendering retires — and a flat profile. Reaching 1× needs ~3.4× more,
which no interpreter lever measured on either Xtensa machine has come
near (the classic's three rungs together were 2.9×): it is the Xtensa
translator keeping a stay's instructions translated (`lp-xt-jit`, today
escaping every instruction), with PGO (opt-in, 1.46× on the classic) as a
desk-only multiplier.

**Incident log**
- 2026-10-22 (#1263, S3 updates M7 P12a) — `walk-ota-ble-emu --s3 --steps
  update` and its USB twin both failed at "the board says what it runs":
  identify had settled on "unrecognized firmware" long before the S3's
  hello. Measured on the desk (load average ≈ 9):
  `lp-cli emu run --chip esp32s3 --host-link --rom-up-flash x.bin --timeout 1s`
  retired 197,826,896 instructions in 41.6 s wall.
- 2026-10-22 (#1270, the paydown above) — profiled: 81 % of host time in
  `BlockCache::invalidate_range`. The same profiling found the cache's
  alias hole (the defect in `related`). Two hosted runs of one binary also
  differ unless the link nonce is pinned (`--link-nonce`): a hosted
  identity check that forgets it reads as a fast path moving the guest.
