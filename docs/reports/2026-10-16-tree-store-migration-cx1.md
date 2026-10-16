# The tree-store cutover's migration on silicon: CX1, littlefs → tree store, every file kept

- **Date:** 2026-10-16
- **Plan:** `lp2025/2026-10-16-0848-tree-store-adoption`, milestone A6
- **Board:** CX1 `c6-expendable` (XIAO ESP32-C6, MAC `14:C1:9F:E6:54:90`,
  chip revision 0.2), leased as `direct: adoption A6`, resolved by MAC
- **Test:** `lp-cli/tests/board_tree_store_migration.rs` (`#[ignore]`d, desk)
- **ADR:** `docs/adr/2026-10-16-c6-tree-store-cutover-migration.md`

Silicon, one run. Not a soak; the emulated walks and the cut sweep are the
breadth (ADR "Verification").

## What ran

1. **Today's littlefs release:** `lp-cli firmware install --release latest
   --mac 14:C1:9F:E6:54:90 --target esp32c6-4mb` put **2026.10.16-1**
   (`3fc9aa987`, the split image on the 176-sector table) on the board. It
   came up `fs: formatted` (the tree store an earlier milestone left there
   was not littlefs) — expected for a board being reset to the release.
2. **Files:** `projects/test/basic` as `basic` and a c40-sized project
   (`basic` plus 124 JSON-like documents) as `c40`, uploaded over USB with
   Studio's replace-and-load, `basic` loaded last. Then the board's raw
   `lpfs` over the bootloader (`lpfs save`), mounted on the host: **142
   files, 222,505 B**, littlefs at `0x350000`; each project's
   `HashPackage` recorded.
3. **Migrate:** `lp-cli hardware lpfs migrate --to tree --yes` with the
   `fs-tree` split package on the tree table (`lp-cli firmware package
   esp32c6-4mb --fs-tree`, commit `b2c28a01a`). It said:

   > Moving 142 files (217 KB) to the tree store at 0x380000: 19 of 128
   > sectors of 4 KB, 109 free. The old filesystem stays whole until the
   > firmware is switched.

   — the **staged** order: the board's own littlefs (written by its
   firmware) left room in the tree region. 45.0 s on the host's wall clock
   for the whole session (inspect, backup stored and read back, write, read
   back, reset, hello). It then verified the hello itself: `fs: mounted`,
   the same board.
4. **After:** the raw `lpfs` again, now `0x380000`, 128 sectors.
   `lp-cli hardware tree check`: **CONSISTENT**, 142 files, 148 records
   hashed, one 42-byte orphan record (garbage until GC). Mounted on the host
   with `lp-tree-store`: **every one of the 142 files byte-identical** to
   the littlefs set. `HashPackage` of both projects **equal** before and
   after (`basic` `65cf8a3f…`, `c40` `b472c578…`).

## The board's own words after the switch

```text
[LOADER] core @0x18000 (proven)
[INIT] Flash filesystem mounted
[FS] tree store mounted (128 sectors, 109 free, root 2)
[CORE] core @0x18000 +1431312 (proven) build b2c28a01a+b2c28a01a1af · loader v1 · region end 0x380000 …
[RECOVERY] boot complete (first frame served)
[shader-node] compilation succeeded (… elapsed=123ms …)
```

The hello lists `fs.tree` and `fs: Mounted`; the region end the core reads
from the table is `0x380000` (the grown `factory`). The loaded project's
output opened (`ws281x:local:D10`, 723 bytes) and its shader compiled on the
second frame, as ever. CX1 carries no LED strip, so "first light" here is
the board's first served frame, not a lit pixel; A1 owns the timed
measurement.

## What it did not cover

- **Boot record 1 was not read back on silicon.** The emulated walk (T1)
  and the emulated CLI test seed a newer record 1 and check it is erased;
  on CX1 the merged image's own bytes (`0xFF` at `0x17000`) were written.
- **No cut on silicon.** The cut sweep is emulated
  (`emu_layout_migration.rs` T5, both orders).
- **One board, one file set.** A crowded littlefs (the retired order) ran
  only emulated.

## A desk finding

`lp-cli firmware install` takes its own lease through `board` and **drops
it when it finishes**, even when the caller already held the board: the
lease went `free` between the install and the next step (the next `board
take` got it back; nobody else was waiting). A caller holding a lease
should keep it.
