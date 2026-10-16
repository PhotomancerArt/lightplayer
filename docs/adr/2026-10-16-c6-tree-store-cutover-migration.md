# ADR: A fielded C6 moves from littlefs into the tree store over USB — staged so the old board stays whole until the switch

- **Status:** Proposed (held for the cutover gate, plan
  `lp2025/2026-10-16-0848-tree-store-adoption`, milestone A6)
- **Date:** 2026-10-16
- **Deciders:** Photomancer (Yona confirms AD1–AD4 at the cutover gate)
- **Amends:** `2026-10-02-c6-repartition-and-layout-migration.md` (the
  migration it built is reused; its write order gains a second shape)
- **Related:** `2026-10-04-c6-split-link-firmware-loader-and-boot-records.md`
  (what a USB flash does to the boot records),
  `2026-10-10-fs-tree-refused-state-and-update-guard.md` (the board-side
  guard in the other direction), `2026-10-06-firmware-distribution.md`
  (`requires.fsTree`, an additive field of format 1),
  `2026-07-28-esp32c6-flash-budget.md` (the 192 KB the smaller `lpfs` frees)

## Context

Every fielded C6 keeps its files — projects, `/hardware.json`,
`/.lp/device.json`, `/.lp/access.json` — in a 176-block littlefs at
`0x350000` (the 2026-10 repartition's table). The tree store
(`lp-base/lp-tree-store`) replaces it, at the 128 sectors it was designed for
(AD2): `lpfs` `0x380000`+`0x80000`, `factory` grown to `0x370000`. The new
region is the old filesystem's blocks 48–175, so the files cannot stay where
they are, and a littlefs image cannot be turned into a tree store by copying
blocks.

The repartition precedent moved files between two littlefs layouts host-side,
in one bootloader session (probe → mount on the host → re-pack → plan →
execute; espflash for `lp-cli`, esptool-js for Studio), with a backup stored
first. Its write order retired the old superblock before writing the new
body, so between the two the backup was the only copy. AD1 keeps that whole
machine; what is new is the target filesystem and what the format of the
tree store allows the write order to do.

## Decision

1. **The same machine, a second target filesystem.** The layout probe reads
   which filesystem the image about to be written mounts from the image
   itself (its manifest core lists `fs.tree`; `layout_migration::fs_kind`),
   and recognises a tree store by any sector's `LTS1` header as well as
   littlefs by its superblock, at the 176- and 128-sector geometries. A board
   on the 176-sector table is an older frozen layout
   (`testdata/partitions-esp32c6-legacy-v2.csv`) and migrates; a board on the
   tree table with a store reads as migrated. The 176-sector table is frozen
   beside the pre-repartition one; the tree table is
   `lp-fw/fw-esp32c6/partitions-tree.csv` until the default switch (A7) makes
   it `partitions.csv`.
2. **The pack is file-level and verified.** Every file goes into a fresh
   128-sector store in ONE transaction (one root, no intermediate ones),
   deflated on the host exactly as a push sends it (`host_deflate_chunks`
   into `put_chunk_deflated`; a file deflate does not shrink, or longer than
   64 chunks, is written whole), with the firmware's `record_max`; the image
   is mounted again with the firmware's own mount and every file compared
   byte for byte. A board whose files do not fit — or would leave fewer than
   16 free sectors — is refused, nothing written. **Empty directories are not
   carried**: the tree store has none (a directory exists while a file is in
   it); every file is.
3. **The write order is staged when it can be.** The tree store is
   position-independent — no sector header or record names its own sector,
   and mount orders sectors by their sequence numbers — so its sectors are
   written into target sectors whose littlefs blocks the old filesystem does
   NOT use (`lpfs_tree::used_blocks`, the blocks a mount, a traversal and a
   read of every file touch). littlefs never reads a block it does not use.
   The order: clean the free sectors the store will not occupy; write the
   store, **the sector holding its root last**; read it back; **disarm** (erase
   the app's first sector at `0x10000`); write the firmware (bootloader, the
   tree table, the app); retire the old filesystem (its superblock pair where
   the image did not cover it, and every target sector it still occupied);
   read the whole region back. A cut before the disarm leaves the **old
   board, working**, every file mounted; between the disarm and a whole
   firmware, a board that boots nothing whose old files are whole (the next
   attempt migrates from them); after the firmware, **the new board**, its
   store complete — the old filesystem's leftovers in its region are
   untrusted to the store (FORMAT.md "Sector", rule 1) and erased.
4. **When the files cannot be staged, the retired order.** A littlefs that
   leaves too few free blocks in the target region, a source that is itself
   a tree store (the 176-sector stores the fs-tree dev builds left; its
   sectors there would be read as the new store's), and a restore from a
   backup: disarm; erase the source and target regions; write the store,
   root last; read it back; write the firmware. Between the erase and the
   root the backup is the only copy, as in the repartition — stated, not
   hidden — and nothing boots in that window. A cut there leaves an old
   table over an erased or rootless region; the probe finds no files to
   migrate, and the stored backup is restored (the resume rule).
5. **The disarm is what makes either order safe against the firmware
   write.** esptool-js and espflash write the merged image from `0x0` up, so
   for the moment between the new table at `0x8000` and the app at
   `0x10000` the old app would boot with the new table — a littlefs core
   that finds no littlefs at `0x380000` and formats over the tree store.
   With its first sector erased first, nothing boots until the firmware is
   whole.
6. **No older core is left in a boot record (AD4).** A USB flash writes the
   split image's merged bytes from `0x0` to the end of `app.bin`: record 0
   (`0x16000`) naming the new core at `0x18000`, proven, and record 1
   (`0x17000`) **erased** — the merged image carries `0xFF` there
   (`lp_fw_split::app_image`), and both flashers erase every sector they
   write. A board that had taken an update over the air (a newer record 1
   naming a high core) comes out with no record of it; the emulated walk
   seeds one and checks it is gone. No plan step is added for it: the
   merged image is the guarantee, and a monolithic image has app code
   there.
7. **A littlefs board is never sent a tree-store core over the air (AD3).**
   The hello's `build.features` reach Studio's device model
   (`HelloFacts::tree_store`); a release says whether it mounts the tree
   store in `requires.fsTree` (ota-manifest and release index, format 1,
   additive: absent reads `false`, which every manifest written before it is,
   and a littlefs build writes nothing, so released bytes are unchanged;
   `lp-cli` writes it from the image's manifest core, and Studio's bundled
   build is refused when the two disagree). A tree-store build for a board
   whose hello lacks `fs.tree` — or that holds its files (`legacy_held`, whose
   older littlefs a core placed high in the app partition could write over)
   — stands `NeedsFilesMoved`: no over-the-air install and no such version
   under "Other version…"; over USB the card keeps its flash
   (`devices/<board>/update-firmware`, Lasting "Replace what this board
   runs?"), which inspects the board and asks before moving its files (the
   question's `continue-update`, Lasting); over a wireless link the card says
   to plug it in.
8. **A tree core holds a skipped migration's littlefs.** An `fs-tree` board
   on the tree table that finds no store, and the 176-sector littlefs whole
   at `0x350000` (a plain flash that skipped the migration), holds it (`fs:
   legacy_held`) instead of formatting over its blocks 48–175, and the probe
   migrates from it. A superblock there survives only while nothing written
   up from `0x10000` reached it, so the blocks after it are ones no image
   wrote. (A7 replaces the read-only mount with superblock magic, AD5.)
9. **Executors.** `lp-cli hardware lpfs migrate --to tree` (the image must
   mount the tree store; `--dry-run` prints every step and writes nothing)
   and Studio's USB update run the same plan through `run_plan` /
   `executePlan`; `lp-cli firmware package --fs-tree` (`LP_FW_IMAGE=split-tree`)
   builds the cutover's image, never a release.
10. **No project-format bump.** Files are carried byte for byte; nothing
    persisted inside a project changes shape. The tree store's own format
    (FORMAT.md, version 3) is what the board holds afterwards.

## Consequences

- **What fits shrinks.** 128 sectors of 4 KB hold about 340 KB of files that
  do not compress (records of 1 KB, three to a sector) and much more of what
  does (projects are JSON and GLSL); a c40-sized board (132 files, 216 KB)
  takes a fraction of it. A board whose files do not fit is refused, as the
  repartition refused; `lp-cli hardware lpfs report` measures one.
- **The probe reads more on a board with no store at the target**: up to 126
  header reads of 24 bytes (one per sector), and the two older layouts'
  heads. A migrated board costs the 3 KB table and its 8 KB head when its
  root is in the first two sectors (the staged plan puts it in the lowest
  free sector), else a header read per sector until one is found.
- **A plain flash of the tree image onto a fielded board stays refused** by
  the layout preflight (`Differs`, both directions); the hold (8) is the net
  under it.
- **A held board's high-core OTA is excluded by Studio, not by the board.**
  Another host could still send a held tree board a core over the air; the
  probe would then trust a superblock whose later blocks a high core wrote
  over. Recorded as a residual.
- The fake device board and Studio's core end-to-end bench still model
  littlefs targets only; the tree route is covered by the standing, offers
  and choices tests and by the emulated walk.

## Alternatives Considered

- **The repartition's order (retire first) for every board.** Simple, and
  the backup covers it — but the director asked for the old filesystem to
  stay mountable until the new store commits, and the store's position
  independence makes that free for every board with room to stage.
- **Staging the store elsewhere** (in the old app partition's free tail):
  the tail's size depends on the image a board runs, and a core placed high
  by an earlier update can occupy it.
- **Writing the firmware first and the store after** (the director's first
  sketch): a cut between them boots the new core over a rootless store,
  which it refuses — safe, but the old board is gone at the first byte; with
  the store first, the old board survives every cut before the switch.
- **An explicit erase of boot record 1 in the plan.** A monolithic image has
  app code there; the split merged image already erases it.
- **On-device migration** (AD1): no spare flash or RAM, not cut-safe.

## Verification

- `lpa-link` unit tests: the probe at both geometries, the pack (fit, too
  tight, deterministic, empty files, hot files, unstorable paths), the
  planner's two orders, and a cut after every step (and inside the firmware
  write at the table) ending old, new, or a board whose next attempt ends new
  with every file (`tree_migration_plan_tests.rs`).
- `lp-cli/tests/emu_layout_migration.rs` (CI "C6 layout migration (x64)"):
  T1–T5 against the pinned fielded image on the emulated C6 through
  espflash's stub, including a cut after every step booted from the reset
  vector.
- `scripts/emu/walk-migration-emu.mjs` T1, T7a/b, T8a/b, T9 (Studio over
  `?emu=`, not CI).
- One silicon run on CX1 (`docs/reports/2026-10-16-tree-store-migration-cx1.md`).
