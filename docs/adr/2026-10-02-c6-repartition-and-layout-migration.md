# ADR: The C6 repartition (3.25 MB app, 704 KB lpfs) and the layout migration that carries every board's files

- **Status:** Accepted (G1, the real-board migration walk, passed 2026-10-03)
- **Date:** 2026-10-02
- **Deciders:** Photomancer
- **Supersedes:** None (spends the reserve `2026-07-28-esp32c6-flash-budget.md`
  Decision 4 held)
- **Superseded by:** None

## Context

The C6 app partition was 3 MB (`factory` `0x10000`+`0x300000`) with `lpfs`
the last 960 KB (`0x310000`+`0xF0000`). `2026-07-28-esp32c6-flash-budget.md`
Decision 4 held that `lpfs` partition as the one reserved lever, to be spent
once, deliberately, on the radio/Wi-Fi decision. The Wi-Fi roadmap's
experiments (`lp2025/2026-10-01-0300-wifi-control-experiments`, decision D1)
measured what LAN control and the relay cost and decided to spend it: the app
grows by 256 KB, the filesystem shrinks by the same.

Every C6 in the field holds user data in that filesystem — projects, the
stamped board manifest `/hardware.json`, the access keys `/.lp/access.json`,
the identity `/.lp/device.json`. A littlefs image cannot be shrunk or moved by
copying blocks, and today's firmware formats any partition it cannot mount.

## Decision

1. **The layout (D1).** `factory` `0x10000`+`0x340000` (3.25 MB); `lpfs`
   `0x350000`+`0xB0000` (704 KB, 176 blocks). The END of the chip stays where
   it was and the filesystem's START moves: the app must grow contiguously from
   `0x10000`, so shrinking from the end is not possible. The new `lpfs` is the
   old one's blocks 64–239. Headroom (`just fw-esp32c6-size-check`): image
   2,966,096 B against 3,407,872 B = **441,776 B** (it was 183,216 B against
   3 MB on this branch before the redraw; 186,496 B at the plan's start).
2. **The old table is a named, frozen fact**:
   `lp-app/lpa-link/testdata/partitions-esp32c6-legacy-v1.csv` (and
   `legacy_c6_v1_table()`), the firmware's `LEGACY_LPFS_V1_OFFSET`. Nothing
   else hardcodes either layout: the firmware, the emulator's direct load, and
   every host tool read the table the chip carries.
3. **The firmware never formats over an old-layout filesystem.** When its
   `lpfs` will not mount and a LightPlayer filesystem is present at the old
   offset, it boots on a memory filesystem and says so (the legacy guard).
4. **The hello says how the filesystem came up**: `fs` =
   `mounted | formatted | memory | legacy_held` (wire proto 34: written as 33, renumbered when main's #929 took 33 first). Studio and
   `lp-cli` verify a migration by it — files mounted and the board's own uid —
   not by "the flash finished".
5. **The migration is file-level, in ONE bootloader session** (not over the
   app wire, which a board on the old firmware cannot be trusted to finish):
   read the table and the old filesystem raw, re-pack every file into a new
   littlefs image at the new geometry in the host (sans-IO, `lpa-link
   layout_migration`), write, read back. Both executors run the same plan:
   espflash for `lp-cli hardware lpfs migrate`, esptool-js for Studio's Update
   firmware.
6. **A backup is stored and read back BEFORE anything is written** (device
   backup archive format 2): Studio's OPFS store `device-backups/` (index
   `version: 1`), or the CLI's backup directory. A browser that cannot store it
   asks for the download first. Between retiring the old filesystem and the new
   superblock landing, that backup is the only copy — stated, not hidden.
7. **The write order** (MQ9): firmware → retire the old superblock pair →
   erase the new superblocks → write the body → write the superblocks last →
   verify → reset; one retry of the tail on a mismatch. The retire moves first
   when the image reaches `0x310000`. A pull before the retire leaves the old
   files intact (the guard holds them); a pull after leaves a board that
   formats, with the backup offered back ("Restore files").
8. **A board whose files do not fit is refused, nothing written** (with fewer
   than 16 free blocks after the re-pack). No selective carry.
9. **No project-format bump.** Files are carried byte for byte; nothing
   persisted changes shape. This ADR is the record of the layout change.
10. **The flash paths preflight the table** both ways (`lp-cli` flash paths,
    `just flash-fw-esp32c6`): a mismatch is refused without `migrate=1` /
    `discard=1`. That catches the downgrade hazard — a pre-repartition image
    flashed onto a migrated board would format its files.
11. **The image may grow past `0x300000` only after Studio can import a
    backup ZIP** (owed): until then the size check prints how far the image is
    from the old filesystem (`legacy overlap: … B`), informational. **Met by
    PR A of `lp2025/2026-10-05-1903-wifi-link-c6` (P01) on 2026-10-05**:
    a device card can now open any backup ZIP Studio ever wrote — including
    one this browser never stored itself — and restore it, which is what an
    interrupted migration past this line needs. The size check's
    legacy-overlap line stays informational: it is a distance to watch, not
    a gate.

## Consequences

- 704 KB of user content per C6 (the catalog's projects measured well under;
  `lp-cli hardware lpfs report` measures a board). Over-full boards are refused
  until a project is removed.
- Every fielded board migrates on its next Studio update; a board that never
  updates keeps working exactly as before. An update now asks a question when
  files move, and takes about a minute longer.
- `/hardware.json`'s stamp and every other file survive byte for byte
  (emulator walk `docs/reports/2026-10-02-c6-migration-emulator-walk.md`).
- The classic (`esp32v3`) keeps the 3 MB / 960 KB table; the S3 is untouched.
- The emulator found two pre-existing defects on the way (the host read's
  in-flight race; the emulated power cycle's strap), both fixed.

## Alternatives Considered

- **Raw image copy** of the old filesystem: cannot shrink, and the new start
  overlaps the old body.
- **On-device migration**: no spare flash to stage into, and not enough RAM to
  hold the files.
- **Keep the start, shrink from the end**: impossible — the app grows from
  `0x10000` and needs contiguous flash.
- **Migrate over the app wire** (read files, flash, push files back): needs a
  healthy old firmware and a second session; a failure between them strands
  the files on the host with no protocol to finish.
- **A selective-carry UI** for over-full boards: deferred (MQ5).

## G1 outcome (2026-10-03)

The real-board migration walk passed. Yona ran the measurement and the
happy path on his own boards, and walked scene 1 (inspect/measure) live on
the spare XIAO C6 (`10:bd:a3:b0:8e:30`); an agent rehearsed scenes 2–5
(refused, bypassed, interrupted, happy path) on that same spare over real
Web Serial; scene 6, the PLAYFUL choker update on Yona's bench C6, read
back afterwards with every one of its 37 files present and only the
expected ones changed (`/hardware.json`'s re-stamp, `/.lp/access.json`'s
key add). The walk found and fixed, on silicon, what no emulator run had
shown: a macOS Web Serial read stall on erased flash (Chromium drops
`0xFF`-heavy bytes a page reads late; fixed in the executor and modelled
into the emulator so `walk-migration-emu` now reproduces it), a wire-32
board reading as "pre-hello firmware" instead of "older LightPlayer" (the
wire-33 hello's required `fs` field failed the whole decode; fixed by
reading `proto` and the board id out of an undecodable hello, in both
Studio and `lp-cli`), a full access store silently dropping the list
Studio had just read when its own sync add was refused (the panel showed
"Who has access 0" instead of what the board actually held), a held board
running a fresh access store in RAM and granting Bluetooth on in the
process instead of failing closed (now locked, and Studio neither syncs
nor lists access for it), and a refusal whose numbers were in the wrong
unit (bytes dressed as if they were the planner's blocks, which could
understate what a board needed to drop). Yona's Q6 (copy) answers were
implemented directly: the card now names the board from an older hello
instead of offering a picker, the card names each step of an update
(reading, waiting for an answer, flashing, moving files, checking) instead
of "Flashing firmware…" throughout, and the files sheet's Continue acts on
one press instead of arming a second confirmation inside a surface that
already asked the question (landed in `828dcb97c`).

## Follow-ups

- 2026-10-04: the split image's high-end cores reuse `0x310000–0x350000`,
  the old `lpfs` head — see
  `2026-10-04-c6-split-link-firmware-loader-and-boot-records.md`,
  Consequences.

- Studio ZIP-file import of a device backup — before the image crosses
  `0x300000` (Decision 11).
- `docs/defects/2026-10-02-a-closed-tab-mid-stamp-leaves-hardware-json-truncated.md`
  (fixed, journaled stamp).
- `docs/defects/2026-10-02-updating-a-tab-hosted-board-erases-its-files.md`
  (open; a tab-hosted emulated board's update erases its whole chip, on
  main too — not specific to this repartition).
