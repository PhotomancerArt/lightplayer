# The tree-store cutover's migration, walked on the emulated C6

- **Date:** 2026-10-16
- **Plan:** `lp2025/2026-10-16-0848-tree-store-adoption`, milestone A6
- **Configuration:** `lp-emu:esp32c6:t1`, ROM-up, at branch commit
  `b2c28a01a` (images built from it). Emulated, not hardware-validated; the
  silicon run is `2026-10-16-tree-store-migration-cx1.md`.
- **The fielded board:** `ReferenceImage::FIELDED_LITTLEFS` (`3fc9aa987`,
  littlefs on the 176-sector table) and a fixture filesystem at `0x350000`.
- **The target:** the `fs-tree` split image on the tree table
  (`lp-fw/fw-esp32c6/partitions-tree.csv`).

## `lp-cli` through espflash's stub (`lp-cli/tests/emu_layout_migration.rs`)

| Test | What | Result |
|---|---|---|
| T1 | the walk's files + a c40-sized project (155 files): `migrate --to tree`, a newer boot record 1 seeded first | **pass**: every file byte-identical in the tree store; boot record 1 erased; old superblock pair erased; the board boots `[FS] tree store mounted`. Retired order (the fixture's littlefs takes nearly every block). 134 s emulated |
| T2 | already on the tree table, files in a store | **pass**: "do not need to move", chip byte-identical |
| T3 | 380 KB of noise that fits 176 littlefs blocks, not 128 sectors | **pass**: refused ("blocks of 4 KB"), chip byte-identical |
| T4 | `--dry-run` | **pass**: every step printed, chip byte-identical, no backup |
| T5 | a cut after every step, booted from the reset vector, both orders | **pass**: staged (8 steps) — 4 cuts the old board working, 4 the new, 2 boot nothing and the next attempt ends new; retired (7 steps) — 1 old, 7 boot nothing (each next attempt, from the backup, ends new with every file), 1 new |

12 min 17 s for the five on an M2 Max.

## Studio over `?emu=` (`scripts/emu/walk-migration-emu.mjs`)

The Studio bundle's C6 package was the `fs-tree` split on the tree table
(`LP_FW_IMAGE=split-tree just studio-firmware-package-esp32c6`), headless
Chrome, the emulator door.

| Walk | What | Result |
|---|---|---|
| T1 | 155 files: Update → question → Continue | **pass**: every file in the tree store; tree table; old littlefs retired; boot record 1 erased; the board mounts its store as itself (uid); access rows the board's |
| T7a | the walk's files only (staged): cable pulled while the store's sectors were written | **pass**: the board came back as itself on its old firmware, littlefs mounted, every file there, the chip still on the 176-sector table |
| T7b | …Update again | **pass**: every file moved; the board mounts its store (120 free sectors) |
| T8 | a crowded littlefs (retired order): cable pulled while the store was written; the card's way back | **pass**: the board booted nothing; the card offered Install (`flash`); the update asked "Put this board's files back" and put every file into the tree store from this browser's backup |
| T9 | the tree image written over `0x0` with no migration | **pass**: the tree core HELD the littlefs (`fs: legacy_held`, Bluetooth off, no access list); Finish update moved every file (`/.lp/access.json` changed by Studio's own key add after the move) |

## Findings

- **The card says "Flashing firmware…" while a staged store is written**
  (T7a's pull read it there): the store's sectors are a few short writes
  ahead of the firmware, and the card did not show "Moving files…" for
  them. Words only; the walks record the sequence.
- **The host-built littlefs fixture is crowded where a firmware-written one
  was not**: the same c40-sized file set staged on CX1 (19 sectors) and
  went retired here. Either order is cut-safe; the staged one keeps the old
  board whole.
- The packaging of an `fs-tree` image failed, and `image_fs_kind` read it
  as littlefs, until the manifest scanner skipped the update guard's own
  marker constants — and the guard itself reads a core that carries no
  manifest blob: `docs/defects/2026-10-16-the-fs-tree-update-guard-reads-a-manifest-the-core-does-not-carry.md`
  (open, A7's).
