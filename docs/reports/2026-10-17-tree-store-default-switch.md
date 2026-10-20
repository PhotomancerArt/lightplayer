# The tree store as the C6's default filesystem: walks, CX1, size

- **Date:** 2026-10-17
- **Plan:** `lp2025/2026-10-16-0848-tree-store-adoption`, milestone A7 (the
  default switch), held with A6 for the cutover gate — PR #1167, which carries
  #1151's commits
- **ADRs:** `docs/adr/2026-10-16-c6-tree-store-cutover-migration.md` and
  `docs/adr/2026-07-28-esp32c6-flash-budget.md` (amendments of 2026-10-17)

Every number below names where it came from. Emulated figures are
`lp-emu:esp32c6:t1` (never hardware-validated); silicon is CX1 only.

## Size (`just fw-esp32c6-size-check`, split image, `esp32c6,server`)

| | main `c20bf9a40` (CI), littlefs, `factory` `0x340000` | `a86ff3aa7` (local), tree store, `factory` `0x370000` |
|---|---:|---:|
| core / engine | 1,421,776 / 1,867,066 B | 1,438,304 / 1,866,016 B |
| **gated headroom** | **66,246 B** | **263,904 B** |
| update headroom | 491,520 B | 688,128 B |
| to the core's next 32 KiB page | 20,016 B | 3,488 B |
| legacy overlap | crossed `0x310000` | 65,536 B before `0x350000` |

littlefs out of the core (the probe and the adapter): core −5,024 B against
the `fs-tree` image on the old table with the probe (1,443,328 B). The
image links no littlefs symbol (`rust-nm`). `core.bin` carries the tree-store
mark (`lp-fw-split`: "carries the tree-store mark (its manifest lists
fs.tree)"); the release's `ota-manifest.json` says `requires.fsTree: true`.
(Main has moved since — the cache reads, the mount index and compile in
frame 1 merged in; CI's "Firmware size" on the PR head is the current number.)

## Emulated walks (all on the default build)

| walk | result |
|---|---|
| `emu_ota.rs` — every OTA scenario, X and Y both tree-store split images (`just test-emu-c6-ota`) | **13/13 pass** (U7 a named skip). U1 X→Y installs a tree core over a tree board: the guard defect is closed — before the fix every tree core was refused |
| `emu_split_scenarios.rs` (`just test-emu-c6-split-boot`) | **12/12 pass** (S6, S10 named skips); S11 now: a core that dies on trial rolls back to one that mounts the store, every file kept |
| `emu_tree_store_boot.rs` | **8/8 pass**: 128 sectors; a littlefs in `lpfs` itself is **held** (byte-identical); a skipped migration's littlefs at `0x350000` is **held**, and a `0x310000` chip formats (not probed) |
| `emu_littlefs_on_tree_store.rs` — the pinned fielded littlefs image (`1d5840a76`) on its own table over a migrated store | **3/3 pass**: it holds (`fs: refused`), writes nothing across a power cycle; the USB probe refuses it and takes the shipped image |
| `emu_batch_push.rs` | **2/2 pass**: the push is one committed batch; a cable cut mid-push leaves version 1 byte for byte (on littlefs it left half of version 2) |
| `emu_stack_peak.rs` | **pass** after the inflate fix: meteor load peak 39,368 → **35,752 B** (main's littlefs figure), margin 13,304 B |
| `emu_seam_led`, `emu_split_boot`, `emu_flash_cut_smoke`, `emu_frag_reads` | pass |
| `walk-migration-emu` T1, T7a, T7b, T8, T9 (real Studio, headless, `?emu=`) | **all pass**: T1 155 files moved byte for byte (one extra, the board's own `/.lp/status-light.json`), tree table, old littlefs retired, boot record 1 erased; T7a a cut while the store is staged leaves the old board, littlefs mounted; T7b finishes it; T8 the retired order's cut, restored from the browser's backup; T9 a plain flash of the tree image **holds** the littlefs at `0x350000` (`fs: legacy_held`) and Finish update moves it |
| `walk-ota-emu` (Studio, over-the-air, X and Y both tree-store, `55f27361b`) | **all 7 steps pass**: update (backup → update → finish, project kept), cut mid-core and mid-engine finished with no click, engine-less restored on connect, can't-get → Install, a pre-update single image left on the USB flash. The first run failed two steps, both bugs the switch exposed and fixed here: Studio's own tree-store build failed its own facts check (`HostBuild::facts()` dropped `requires.fsTree`: every Studio OTA ended at once with `MissingEngine`; main's walk at `1d5840a76` passes the same step), and an engine-less board (no hello, so no `fs.tree`) was held back from the tree-store build |
| `walk-esp32c6-emu` (the C6 hardware walk's twin) | **pass**: the frame byte-identical on all three readings; t1 186.6 fps, t3 65.2 fps |
| first light, `report-first-light-emu t3` (from reset, report-only) | no project: none by 263 ms; `basic` **522 ms** (fs 6, load 83, lit 181); c40 **537 ms** (fs 11, load 93) — with #1160/#1161/#1165 merged |

The migration walk's step hooks went stale with main's card words round 2
(#1159: the Firmware row says "installing · N%", not each step); the walk now
reads that row's verb, and its cable cuts still key off esptool-js's own
`Writing at` lines.

## CX1 (silicon)

`c6-expendable`, MAC `14:C1:9F:E6:54:90`, leased as `direct: adoption A7`,
resolved by MAC, power only through `board power-cycle`.

1. Release **2026.10.16-16** (`e236158d2`, littlefs, with #1150's hold)
   installed with `lp-cli firmware install --release latest` (its `lpfs`
   region and the app's first sector erased first: an earlier sitting had
   left a tree store there, which the release's own preflight refused to
   flash over — as it should).
2. `basic` + a c40-sized project uploaded: **142 files, 222,505 B**.
3. **10 power-on boots on littlefs**, then `lp-cli hardware lpfs migrate --to
   tree --yes` with this branch's package (`30fb37f8e`): staged, "19 of 128
   sectors … 109 free. The old filesystem stays whole until the firmware is
   switched", **44.6 s** host wall clock; then **10 power-on boots on the tree
   store**.
4. After: `hardware tree check` **CONSISTENT** (142 files, one 42-byte
   orphan); every file **byte-identical**; both `HashPackage`s equal; the
   hello lists `fs.tree`, `fs: Mounted`; `[CORE] … region end 0x380000`.

First light (the firmware's own `[perf] first light` line, ms):

| | median of 10 | range | fs | load | lit |
|---|---:|---|---:|---:|---:|
| littlefs (release) | **677** | 676–683 | 8 | 113–115 | 194–196 |
| tree store (`30fb37f8e`) | **683** | 679–684 | 18 | 118–119 | 183–187 |

The tree store is 6 ms slower to first light on this board and file set
(mount 18 vs 8 ms, load +5 ms, lit −9 ms). `30fb37f8e` predates main's cache
reads (#1161), mount index (#1160) and compile in frame 1 (#1165), all now
merged into this branch, and the inflate fix; this run does not measure them.
