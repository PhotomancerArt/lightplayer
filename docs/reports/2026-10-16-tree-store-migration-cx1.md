# The tree-store cutover's migration on silicon: CX1, littlefs → tree store, every file kept

- **Date:** 2026-10-16 (run at 21:47 PT; an earlier run the same day, at
  `b2c28a01a`, is summarised at the end)
- **Plan:** `lp2025/2026-10-16-0848-tree-store-adoption`, milestone A6
- **Board:** CX1 `c6-expendable` (XIAO ESP32-C6, MAC `14:C1:9F:E6:54:90`,
  chip revision 0.2), leased as `direct: adoption A6`, resolved by MAC
- **Test:** `lp-cli/tests/board_tree_store_migration.rs` (`#[ignore]`d, desk)
- **ADR:** `docs/adr/2026-10-16-c6-tree-store-cutover-migration.md`

Silicon, one run. Not a soak; the emulated walks and the cut sweep are the
breadth (ADR "Verification").

## What ran

1. **Today's littlefs release:** `lp-cli firmware install --release latest
   --mac 14:C1:9F:E6:54:90 --target esp32c6-4mb` put **2026.10.16-12**
   (`481072bdd`, the split image on the 176-sector table) on the board. It
   came up `fs: mounted` on the littlefs it found (a `first-light` project
   an earlier sitting left), and the lease stayed held through the install
   (the fix in this PR; before it, the install dropped it).
2. **Files:** `projects/test/basic` as `basic` and a c40-sized project
   (`basic` plus 124 JSON-like documents) as `c40`, uploaded over USB with
   Studio's replace-and-load, `basic` loaded last. Then the board's raw
   `lpfs` over the bootloader (`lpfs save`), mounted on the host: **150
   files, 229,763 B**, littlefs at `0x350000`; each project's
   `HashPackage` recorded.
3. **First light on littlefs:** a power cycle (`board power-cycle`) and the
   board's own line:

   > `[perf] first light at 691 ms: boot 166 init 22 fs 9 radio 86 split 4 server 7 load 140 frame1 71 lit 186`

4. **Migrate:** `lp-cli hardware lpfs migrate --to tree --yes` with the
   `fs-tree` split package on the tree table (`lp-cli firmware package
   esp32c6-4mb --fs-tree`, commit `cfe74b783` — main with #1149's lazy
   format, #1154's fast mount and #1158's format-v3 provisions merged). It
   said:

   > Moving 150 files (224 KB) to the tree store at 0x380000: 19 of 128
   > sectors of 4 KB, 109 free. The old filesystem stays whole until the
   > firmware is switched.

   — the **staged** order. 44.6 s on the host's wall clock for the whole
   session (inspect, backup stored and read back, write, read back, reset,
   hello). It verified the hello itself: `fs: mounted`, the same board.
5. **After:** the raw `lpfs` again, now `0x380000`, 128 sectors.
   `lp-cli hardware tree check`: **CONSISTENT**, 150 files, 148 records
   hashed, one 42-byte orphan record (garbage until GC). Mounted on the host
   with `lp-tree-store`: **every one of the 150 files byte-identical** to
   the littlefs set. `HashPackage` of both projects **equal** before and
   after (`basic` `65cf8a3f…`, `c40` `b472c578…`).
6. **First light on the tree store:** a second power cycle:

   > `[perf] first light at 681 ms: boot 166 init 22 fs 18 radio 84 split 3 server 7 load 118 frame1 77 lit 186`

   One boot each, so the 10 ms is not a finding; the parts say the same
   thing A1/A8 measured over many boots: the mount is not where first light
   goes (`fs` 9 → 18 ms, `load` 140 → 118 ms).

## The board's own words after the switch

```text
[LOADER] core @0x18000 (proven)
[INIT] Flash filesystem mounted
[FS] tree store mounted (128 sectors, 109 free, root 2)
[CORE] core @0x18000 +1441648 (proven) build cfe74b783+cfe74b783b1f · loader v1 · region end 0x380000 · page 0x8000 · engine 1865548 B · digest ffa4ff2c
[shader-node] compilation succeeded (… elapsed=121ms …)
```

The hello lists `fs.tree` and `fs: Mounted`; the region end the core reads
from the table is `0x380000` (the grown `factory`). CX1 carries no LED
strip; the firmware's first-light line is said at the first frame with a
lit pixel in its output buffer, strip or not.

## What it did not cover

- **Boot record 1 was not read back on silicon.** The emulated walk (T1)
  and the emulated CLI test seed a newer record 1 and check it is erased;
  on CX1 the merged image's own bytes (`0xFF` at `0x17000`) were written.
- **No cut on silicon.** The cut sweep is emulated
  (`emu_layout_migration.rs` T5, both orders).
- **One board, one file set.** A crowded littlefs (the retired order) ran
  only emulated.

## The earlier run (same day, `b2c28a01a`)

Release 2026.10.16-1 plus the same two projects: 142 files, 222,505 B,
staged (19 of 128 sectors), every file byte-identical, both `HashPackage`s
equal, `tree check` consistent, 45.0 s. It read no first-light line; this
run adds both, and runs the store with main's later changes. It found that
`lp-cli firmware install` dropped a lease its caller held — fixed in this
PR (`board_bench::held_by`).
