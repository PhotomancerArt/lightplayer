# The tree store in the C6 firmware, walked and cut on the emulator (M5)

Plan `lp2025/2026-10-08-2339-tree-store-firmware-and-emulator`, milestone M5
of `lp2025/2026-10-08-1017-tree-store-device-round`. PR B (#1131).

**Everything here is emulated** — `lp-emu:esp32c6:t1+net=lan` (cuts:
`…+flash-cut`), lp-emu at `535f7d135` (its flash engine and flash-cut
injection as merged in `5c78fcd06`, PR A), firmware built from this branch,
an M2 Max host. **Nothing here is hardware-validated.** The emulator's flash
commands take no time (`docs/debt/emulated-flash-commands-take-no-time.md`):
every emulated time below is CPU only, and every flash-time figure is a
labelled silicon **estimate**. M7 measures on CX1.

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
  (D1, wire 42). The store's code appears once in the ELF: 36 symbols name
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

## Adoption conditions M5 hands on

1. **littlefs and the legacy probe leave the image, or the store shrinks by
   ~10 KB (D7).** Measured, the `fs-tree` image already clears the 64 KB
   floor with the probe (gate 81,114 B, core +10.3 KB), so this condition
   as written is met by the numbers; what remains of it is the D8 hazard (below) and the probe's
   own question (the legacy layout it protects is not reachable on today's
   image).
2. **A lazy or incremental format (D4).** Yona, 2026-10-10: "not the end of
   the world, but might be worth trying to improve before it goes live."
   The store already erases and verifies a sector before opening it, which
   is where a lazy format would land its erase; decided on M7's number.
3. **The two-way littlefs/tree-store guard in the product image (D8):** a
   littlefs image that holds on tree-store magic instead of formatting.
   This PR guards only the other direction.

## What none of this proves

No radio (Wi-Fi not joined, so the heap is not silicon's), no flash timing,
no silicon, no real USB stack, no CX1. The emulator's weak bits persist in
process only (Q8). A fidelity gap found on the way:
`docs/defects/2026-10-10-the-emulated-c6-has-no-amomin-or-amomax.md`.
