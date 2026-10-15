# The tree store in the C6 firmware, walked and cut on the emulator (M5)

Plan `lp2025/2026-10-08-2339-tree-store-firmware-and-emulator`, milestone M5
of `lp2025/2026-10-08-1017-tree-store-device-round`. PR B (#1131).

**Everything here is emulated, except the section "On CX1"**, which holds
silicon numbers from 2026-10-14. The rest is `lp-emu:esp32c6:t1+net=lan` (cuts:
`…+flash-cut`), lp-emu at `535f7d135` (its flash engine and flash-cut
injection as merged in `5c78fcd06`, PR A), firmware built from this branch,
an M2 Max host. **Nothing here is hardware-validated.** The emulator's flash
commands take no time (`docs/debt/emulated-flash-commands-take-no-time.md`):
every emulated time below is CPU only, and every flash-time figure is a
labelled silicon **estimate**. M7 measured on CX1 (see "On CX1" below).

## What `fs-tree` is

A non-default `fw-esp32c6` feature (never in `served.json`, a release or
`bless-chips`; `lpa-boards`' drift gate fails if a served build turns it on)
that makes `lpfs` the tree store (`lp-base/lp-tree-store`) over esp-storage
through a word-alignment shim (`fw_esp32_common::aligned_nor_flash`), with
the SHA accelerator as its hasher. Every board file goes through it.

- **Mount verdicts** (`mount_verdict.rs`, D2): `NoStore` (no trusted header
  of this format, or only an interrupted first format's empty directory) is
  formatted, unless the legacy probe holds; `Damaged` and `Unsupported` are
  **refused**: nothing written, files kept, `fs: refused`, access locked
  (D1, wire 43). The store's code appears once in the ELF: 36 symbols name
  one `TreeStore<AlignedNorFlash<FlashStorage>, StoreSha256>`, and the
  size probe shows one `TreeStore<RamFlash, ProbeHasher>` and no `&mut`
  instantiation.
- **Update guard** (D8): the split build's update session refuses a core
  whose embedded manifest lacks `fs.tree`.
- ADR: `docs/adr/2026-10-10-fs-tree-refused-state-and-update-guard.md`.

## Boot walks (`emu_tree_store_boot`, P6)

| case | board said | flash |
|---|---|---|
| blank | `formatted`; `[FS] tree store formatted (176 sectors, 174 free, root 1)` | host mount: empty store |
| upload basic, power-cycle | `mounted`; `[FS] tree store mounted (176 sectors, 172 free, root 10)` | 8/8 files byte for byte over the wire and on the host mount; package hash equal |
| a littlefs partition | `formatted` | the littlefs files gone |
| legacy layout | **not reachable on today's image** — see below | — |
| newer header | `refused`; `… — files kept (newer format); using memory FS, access locked`; `[ble] off (file store refused …)`; network writes refused | lpfs byte-identical |
| damaged (no root) | `refused` (`no complete root`), the rest as above | lpfs byte-identical |
| project switch + panel write | `mounted`; boots the switched-to project; `panel.json` holds `{"f32":42.5}` | — |
| split image | `formatted` → upload → `mounted (…, root 11)` | files on the host mount |

Making the firmware format on every mount error failed the two refused
cases (checked, reverted).

**Legacy layout:** a direct load stages the app in `factory`, and today's
`.text` runs past `0x310000`, over the pre-repartition filesystem's
superblock pair, so **today's littlefs image formats that chip too**. The
case asserts `fs-tree` does what the shipped image does there and says so;
the probe-and-hold path is the host test `tree_fs::tests::hold_writes_nothing`.

## Cut-and-reboot walks (`emu_tree_store_cuts`, P7)

The oracle (D3), both layers required: the cut image, mounted on the host
with `lp-tree-store`, is a root the dry run committed between the last
acknowledged step and the one in flight, with exactly its files; the
rebooted board says `mounted` (or `formatted|mounted` for the first-boot
format), serves exactly what its flash holds, and that equals the cut image;
the project `/lightplayer.json` names runs a package hash that slot had, and
a panel value is one that was written. Sampling (D6): first and last op,
the erases, the two programs before each root, a seeded sample. Models: all
nine named (`calibrated`, its five forced erase shapes, `clean`,
`byte_prefix`, `random_bits`). A deliberately broken window check failed
22/22 save cuts and `REPLAY=` reproduced one (reverted).

**fs-tree, 176 sectors (long form)**

| scenario | T (in-range ops) | erases | cuts | failures | landed old / between / new |
|---|---:|---:|---:|---:|---|
| format (first boot) | 360 | 176 | 1,728 | **0** | 1,727 / 0 / 1 |
| push (basic, two slots) | 169 | 1 | 329 | **0** | 67 / 262 / 0 |
| push-c40 (132 files, ~216 KB) | 15,392 | 110 | 2,425 | **0** | 37 / 2,388 / 0 |
| save | 38 | 1 | 123 | **0** | 123 / 0 / 0 |
| panel (two writes) | 21 | 0 | 68 | **0** | 67 / 0 / 1 |
| switch | 11 | 1 | 42 | **0** | 42 / 0 / 0 |
| **total** | | | **4,715** | **0** | |

**fs-tree, 128 sectors** (a fixture table; the firmware and the cut read it
at runtime): format 1,296 (T = 264, 128 erases), push 329, save 123, switch
42 — **1,790 cuts, 0 failures**.

**CI subset** (`test-emu-c6-cli-boards`): format 51, push 63, save 22,
panel 6, switch 16 — **158 cuts, 0 failures**, 123 s wall locally with 8
threads; CI's 4-core runner should take roughly twice that.

**fs-tree overall: 6,663 cuts, zero failures.**

**littlefs control** (today's shipped image, report-only, 176 sectors):

| scenario | cuts | failures | |
|---|---:|---:|---|
| push (basic) | 643 | 9 | all one op (9), every model: a created file left **empty** |
| save | 292 | 0 | |
| switch | 46 | 0 | |
| panel (CI form) | 30 | 9 | the first `panel.json` left empty |
| **total** | **1,011** | **18** | one mechanism, filed: `docs/defects/2026-10-10-a-cut-during-a-littlefs-file-create-leaves-it-empty.md` |

The control's push-c40 was not run (littlefs cannot hold two c40 copies;
see RAM), nor format.

**Amendment, 2026-10-14:** the empty-file residue is fixed in today's
littlefs firmware (PR #1134: a new file is written under a hidden name and
renamed into place within its directory). Re-run on one machine: `main`
`dc5164ef1` panel 9/30 (CI form) and 21/184 (long), push 0/649 (it no
longer reproduced there); with the fix, push 736, save 292, switch 70, panel
37 and 196 cuts, **0 failures**. The defect entry has the table.

## RAM (`emu_tree_store_ram`, P8, D5)

Unsplit images with `heap_peak_diag` (the split ROM-up walk outran its
10-minute command: a deviation), requested bytes (allocator overhead is
`max_usage`), a c40-shaped project stored and loaded at boot. **The emulated
C6 has no Wi-Fi joined**: compare the two rows, not either with silicon's
38–46 KB largest block.

| | resident live | max_usage | largest at rest | save | panel write | 12 KiB in 3 chunks | overlap peak | overlap min largest | boots |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| fs-tree | 212,028 | 251,804 | 37,656 | 31,664 | 6,513 | 39,154 | 256,080 | 34,072 | 1 |
| littlefs | 206,384 | 254,792 | 46,288 | 31,128 | 6,513 | 39,286 | 252,278 | 33,300 | 1 |
| fs-tree − littlefs | **+5,644** | −2,988 | **−8,632** | +536 | 0 | −132 | +3,802 | +772 | |

- **Resident:** the store costs 5.6 KB more than littlefs at c40, inside D3's
  "≤ 8 KB + littlefs's own resident".
- **Transient:** the per-operation peaks (~31 KB a save) are the request
  path's (decode buffers, the 4 KiB chunk, the reply), the same for both;
  the store's own share is the difference, ≤ +0.5 KB, inside D3's 5 KB.
- **Overlap** (a c40 push whose load compiles, then two panel writes): no
  reset, no failed allocation; the lowest sampled largest block 34,072 B vs
  littlefs's 33,300 B.
- **Finding:** at rest the largest free block is **8.6 KB smaller** with the
  tree store (37,656 vs 46,288 B) — its resident allocations land between
  bigger holes. On a joined silicon C6 (38–46 KB) that matters; M7 should
  read it there.
- **Finding:** littlefs cannot hold a second c40 copy (`no space left on
  device` at the 27th document: every file over its 512 B inline limit
  takes whole 4 KiB blocks); the tree store holds both. The overlap pushes
  into the same slot for both.

## Perf (`emu_tree_store_perf`, P8, Q17/Q18)

Emulated µs are **CPU only**; round trips include the link and the server's
tick. Silicon estimate = erases × 21 ms + program bytes / 256 × 0.5 ms
(ESTIMATE, flash only).

| operation | fs-tree µs | littlefs µs | fs-tree programs / bytes / erases / SPI1 reads | littlefs programs / bytes / erases / reads | silicon est. ms fs-tree / littlefs |
|---|---:|---:|---|---|---|
| first boot (format) | 68,246 | 685 | 184 / 4,336 / 176 / 46,673 | 4 / 128 / 2 / 678 | **3,704** / 42 |
| mount, empty | 3,723 | 233 | 0 / 0 / 0 / 1,122 | 0 / 0 / 0 / 525 | — |
| push c40 and load | 2,305,000 | 13,012,750 | 15,367 / 415,896 / 110 / 109,989 | 16,791 / 534,096 / 401 / 2,194,394 | 3,123 / 9,464 |
| mount, c40 | **277,785** | 3,116 | 0 / 0 / 0 / 38,964 | 0 / 0 / 0 / 6,005 | — |
| save (1.7 KB), each | 88,000 | 115,250 | 89 / 2,443 / 0 / 1,344 | 63 / 2,000 / 2 / 9,781 | 5 / 46 |
| 4 KiB append (+ `file_size`), each | 87,312 | 89,312 | 33 / 820 / 0 / 1,282 | 172 / 5,480 / 3 / 4,951 | 2 / 74 |
| read the largest c40 file | 35,700 | 34,850 | 0 / 0 / 0 / 473 | 0 / 0 / 0 / 1,415 | — |
| whole-project load (c40) | 71,250 | 75,750 | 0 / 0 / 0 / 1,077 | 0 / 0 / 0 / 4,661 | — |
| mount, full | 330,586 (c40 + 60 × 4 KiB) | 3,605 (c40 + 28: littlefs said no space) | reads 47,198 | reads 5,991 | — |

**Q18: not tripped.** The c40 whole-project load is 0.94x littlefs's and a
4 KiB append with its `file_size` walk 0.98x, against the 1.5x bar: no
directory cache was built.

**Finding:** the tree store's mount is **~89x littlefs's** in emulated CPU at
c40 (278 ms vs 3 ms; 331 ms nearly full), and its first boot's format is
3.7 s of erases on silicon (estimate). Both are boot-time and before the
radios; M7 should measure them on CX1, with any watchdog.

## Size (P9; local macOS builds, split image, all after `main`'s #1092)

| image | core B | engine B | app.bin B | image headroom | steady | update | **gate** | core slack to its page |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| default, `main` `5d72e82a2` | 1,420,176 | 1,851,530 | 3,330,048 | 77,824 | 81,782 | 491,520 | **81,782** | 21,616 |
| default, this PR `d91351728` | 1,420,224 | 1,851,754 | 3,330,048 | 77,824 | 81,558 | 491,520 | **81,558** | 21,568 |
| `fs-tree` (legacy probe kept) | 1,430,512 | 1,852,198 | 3,330,048 | 77,824 | 81,114 | 491,520 | **81,114** | 11,280 |
| `fs-tree`, no probe (`measure_no_legacy_probe`) | 1,425,376 | 1,852,228 | 3,330,048 | 77,824 | 81,084 | 491,520 | **81,084** | 16,416 |

The core starts at `0x18000` and the engine at `0x178000` in every row:
**no row crosses a 32 KiB page.** The default image moved by +48 B (core) and
+224 B (engine; gate −224 B), app.bin unchanged: the wire's new `refused`
value and the feature registry's `fs.tree`, which every build carries.

**This disagrees with the plan (D7, notes "Size").** The prediction was an
`fs-tree` core ≈ 40–42 KB bigger, a page crossed and the gate ≈ 55.6 KB,
under the 64 KB floor. Measured, the core grows **10,288 B** and the gate is
**81,114 B**, above the floor. By symbol (`rust-nm --print-size`, the
default vs the `fs-tree` p2.elf): `lp_tree_store` +39,652 B, littlefs
−21,558 B (3,754 B left: the read-only probe's mount), `LpFsFlash` −7,980 B,
everything else +979 B. The legacy probe keeps only littlefs's mount path;
LTO drops its writer, its format and the adapter. "littlefs removed" (no
probe) is +5,152 B over the default core (the plan's estimate was
+6.6–8.6 KB).

## D4: the first-boot format, accepted for M5

`format` kills, erases and erase-verifies every sector: 176 erases, ≈ 3.7 s
on silicon (176 × ≈ 21 ms, an ESTIMATE), each erase inside esp-storage's
critical section with interrupts masked; 68 ms of CPU on the emulator, which
cannot see the rest. Accepted for M5 (first boot only, before the radios and
the server); M7 measures it on CX1, including whether a watchdog fires.

## G-M5, answered (Yona, 2026-10-14: "all leans")

1. **Cuts:** the emulated cut evidence is enough to move on to CX1 (M7).
2. **RAM:** not called fine yet. Measure the **at-rest largest free block on
   CX1 with Wi-Fi joined** first (the emulated −8.6 KB was taken with no
   radio).
3. **Perf:** measure **mount** and the **first-boot format** on CX1; the
   emulated figures are CPU only.
4. **Size:** "littlefs must leave the image" is **demoted to a
   nice-to-have**, not an adoption condition (the `fs-tree` image already
   clears the 64 KB floor with littlefs's probe linked).
5. **The list:** complete with the additions below.

## Adoption checklist M5 hands on

1. **A lazy or incremental format (D4), before it goes live.** Yona,
   2026-10-10: "not the end of the world, but might be worth trying to
   improve before it goes live." The ~4 s first-boot format (176 erases,
   interrupts masked) must become lazy before `fs-tree` is the default. The
   store already erases and verifies a sector before opening it, which is
   where a lazy format would land its erase; decided on M7's number.
2. **The two-way littlefs/tree-store guard in the product image (D8):** a
   littlefs image that holds on tree-store magic instead of formatting.
   This PR guards only the other direction.
3. **Mount time, measured on CX1** (emulated: 278 ms CPU at c40), and the
   first-boot format's time there.
4. **The at-rest largest free block, measured on CX1 with Wi-Fi joined**
   (emulated, no radio: 8.6 KB smaller than littlefs).
5. **The unreachable legacy-layout guard.** The probe-and-hold path cannot
   fire on today's image (the app overwrites the old superblock pair at
   `0x310000`, see "Legacy layout" above): make it reachable, or drop it
   with the probe.
6. *Nice-to-have, no longer a condition (G-M5, 2026-10-14):* littlefs and
   the legacy probe leave the image (D7). Measured, the `fs-tree` image
   clears the 64 KB floor with the probe (gate 81,114 B, core +10.3 KB).

## On CX1: mount, first-boot format and free block (silicon, 2026-10-14)

**These numbers are silicon**, not emulated. They answer G-M5's items 2 and 3
and checklist items 3 and 4.

- **Board:** CX1 `c6-expendable`, MAC `14:C1:9F:E6:54:90`. A generic ESP32-C6
  (not a XIAO), chip v0.2, 4 MiB flash at DIO 40 MHz, flashed with
  `lp-fw/fw-esp32c6/partitions.csv` (`lpfs` = 176 sectors at `0x350000`). With
  no `/hardware.json` it runs the compiled-in manifest.
- **Images:** `main` `dc5164ef1d05` (PR #1131), clean. Both are monolithic dev
  images built in `lp-fw/fw-esp32c6` with `--profile release-esp32` and the
  default features plus `esp32c6,server,radio,heap_peak_diag`. That is the
  emulated walk's `SHIPPED_HEAP_PEAK` / `FS_TREE_HEAP_PEAK` pair, unsplit like
  that walk.
  - littlefs: ELF sha256 `9db1bfba…b73b68f`, app 3,226,176 B.
  - `+fs-tree`: `45387c1c…7e00d8`, app 3,241,072 B.
- **Project:** the walk's c40 shape, `c40_files()` ported byte for byte:
  `projects/test/basic` plus 124 seeded documents, 132 files, 215,150 B. It
  was uploaded with `lp-cli upload` as `demo` and is the startup project.
- **Wi-Fi: joined.** Every free-block boot joined the desk's test AP over the
  board's own network file (`lp-cli wifi add … --password-stdin`). Each logged
  `[wifi] connected … -34 to -36 dBm` before the lines that were read. No
  credential is in the repo or the captures.
- **Method:** only lines the firmware already prints. `[perf] fs mount <us> us`
  is the mount timer, and it includes a first-boot format. `[heap] peak=…
  live=… max_usage=… largest=…` is one line per 5 s heartbeat. No logging was
  added.
  - **Format:** `espflash erase-region 0x350000 0xB0000`, then
    `scripts/emu/reset-and-capture.py` (a chip reset with the port held open).
  - **Mount:** chip resets the same way, plus `lp-cli link capture --request
    reboot` software reboots.
  - **Free block:** two protocols.
    - *Host from boot:* `link capture --request reboot`, 100 s, so the host
      is on the link the whole time.
    - *Host late:* a chip reset with nothing on the link, 40 s idle, then a
      40 s `link capture`.
  - In both, a boot's figure is the median `largest` of its `[heap]` lines after
    `[wifi] connected`. The first line after it is skipped, because its window
    still holds the boot's load.
- **Data:** the captures are in `docs/reports/2026-10-14-tree-store-cx1/`, one
  file per boot. `python3 docs/reports/2026-10-14-tree-store-cx1/summarize.py`
  reproduces every figure below.

| CX1, silicon | littlefs | `fs-tree` | `fs-tree` − littlefs | emulated (no radio) |
|---|---:|---:|---:|---|
| **(b) first-boot format**, from erased `lpfs` | 7.38 ms (7.31–11.37, n=6) | **3.316 s** (3.290–3.340, n=5) | +3.31 s | 68 ms CPU; **3.704 s silicon estimate** |
| **(a) mount, c40** (all boots) | 7.47 ms (7.46–8.74, n=20) | **92.70 ms** (92.68–93.97, n=20) | +85 ms (12.4x) | 278 ms vs 3.1 ms (89x) |
| mount, empty (after the format) | 0.74 ms (n=3) | 7.63 ms (n=3) | +6.9 ms | 3.7 ms vs 0.23 ms |
| **(c) largest free block at rest, joined**, host from boot | 33,296 B (19,924–34,980, n=7) | **39,676 B** (39,604–39,876, n=7) | **+6,380 B** | 37,656 vs 46,288: **−8,632 B** |
| (c) the same, host late | 40,952 B (23,692–41,624, n=7) | 38,828 B (35,488–39,512, n=7) | −2,124 B | |
| live (requested) at rest, host from boot | 218,944 B | 221,702 B | +2,758 B | +5,644 B |
| live at rest, host late | 218,934 B | 221,738 B | +2,804 B | |

Medians, then range and count, across boots. The mount timer starts after the
partition table and boot-control reads, so it times only the filesystem.

- **(b) Format: 3.3 s, inside the estimate.** The estimate was 3.7 s (176
  erases at 21 ms). Measured, it is 3.32 s with CPU and erase-verify included,
  about 18.8 ms a sector. No boot reset. The capture has one `rst:` line, and
  the RTC watchdog's boot window is 30 s (`[RECOVERY] RWDT armed: boot 30000
  ms`). Interrupts are masked one erase at a time (~19 ms each, inside
  esp-storage's critical section), not for the whole 3.3 s. The five formats
  grew steadily from 3.290 to 3.340 s. The cause is not known. Checklist item 1
  (a lazy format before going live) stands on this number.
- **(a) Mount: 93 ms at c40, 12x littlefs, not 89x.** Steady to ±15 µs across
  chip resets. A software reboot occasionally adds ~1.3 ms. The emulator
  over-charges it 3x: it reports 278 ms of CPU alone, against silicon's 93 ms
  of CPU and flash. That is filed as
  `docs/defects/2026-10-14-the-emulated-c6-mounts-a-c40-tree-store-three-times-slower-than-silicon.md`.
  The emulated mount figures in "Perf" above should not be read as silicon
  estimates.
- **(c) Free block: the emulated −8.6 KB does not appear on silicon with Wi-Fi
  joined.** The tree store's at-rest largest block is the steadier of the two
  (39.6–39.9 KB in every host-from-boot boot).
  - *Host from boot:* the tree store is 6.4 KB larger than littlefs.
  - *Host late:* it is 2.1 KB smaller, inside littlefs's own spread.
  - littlefs's largest block depends on when the host link comes up (33 vs
    41 KB). One boot under each protocol fell to 20–24 KB.
  - Both images sit in the 38–46 KB band quoted for a joined C6, or below it.
  - The tree store's resident cost is +2.8 KB live on silicon (+5.6 KB
    emulated). The emulated gap was layout, not size. Wi-Fi's allocations
    change where things land.
  - This is not an emulator fidelity defect. The emulated run had no radio by
    design, and it said so.

No other board was touched. The lease was taken, renewed and dropped through
`board`. The power was never switched.

## What none of this proves

The emulated sections prove nothing about the radio: Wi-Fi was not joined,
so their heap is not silicon's. They also prove nothing about flash timing,
silicon or a real USB stack. "On CX1" covers mount, format and the joined
heap at rest on one board. It does not cover cuts, transient RAM or the
overlap on silicon. The emulator's weak bits persist in process only (Q8).
Fidelity gaps found on the way:
`docs/defects/2026-10-10-the-emulated-c6-has-no-amomin-or-amomax.md`, and,
on CX1,
`docs/defects/2026-10-14-the-emulated-c6-mounts-a-c40-tree-store-three-times-slower-than-silicon.md`.
