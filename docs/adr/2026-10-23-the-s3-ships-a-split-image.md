# ADR: The ESP32-S3 ships a split image too — layout 2, on 64 KiB pages, through the IDF door

- **Status:** Accepted
- **Date:** 2026-10-23
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None
- **Related:** the C6's split image,
  [2026-10-04-c6-split-link-firmware-loader-and-boot-records](2026-10-04-c6-split-link-firmware-loader-and-boot-records.md)
  (this ADR is its S3 sibling and changes nothing of layout 1);
  [2026-10-06-ota-update-protocol](2026-10-06-ota-update-protocol.md) (its
  2026-10-23 amendment adds the S3's codes);
  [2026-07-30-esp32s3-partition-floor](2026-07-30-esp32s3-partition-floor.md)
  (its 2026-10-23 amendment: the table stays);
  [2026-10-16-c6-tree-store-cutover-migration](2026-10-16-c6-tree-store-cutover-migration.md)
  (the tree store the S3 now mounts too).

## Context

The C6 updates itself over the air from a split image: a loader, two boot
records, a core that keeps the board reachable, and an engine the core can
rewrite. The ESP32-S3 had no update path at all: a new build took a cable.

There were two ways to give it one. The first was the C6's split image. The
second was whole-image A/B, two complete images that alternate. A/B looked
simpler, but it did not fit the S3's table. The image was 2.45 MB, and two
3.5 MB slots plus a smaller `lpfs` meant a repartition. It would also have
needed a one-image manifest shape and a second update target in the
protocol: a Lasting format of its own, beside the C6's.

A day-long spike on the emulator (planning
`lp2025/2026-10-20-2335-s3-radio-and-ota/spike-s3-split/report.md`, branch
head `e07c495f6`) took the C6's split image to the S3 and through all three
rungs:
- a verified two-pass split link;
- a layout in `lp-bootctl`'s terms;
- the real ROM and the real IDF bootloader starting a loader, which booted a
  core, which entered an engine that compiled and rendered a project.

It found the S3 easier than the C6 in two ways. Every Xtensa call is
`l32r` + `callx8`, so a call has no range. And one MMU table serves both
buses at one index, so the engine is one run of pages mapped once. The real
work was in GNU ld, which behaves in three ways the C6's lld never showed
(§6).

## Decision

### 1. The C6's split image, on the S3

The S3 ships what the C6 ships: one link, cut by reachability into a
**core** and an **engine**, with a RAM-only **loader** and **two boot
records** in front, all inside `factory`. Each piece, its chip-neutral
definition, and the S3-specific part:

| Piece | Shared with the C6 | The S3's own |
|---|---|---|
| The split link | `tools/lp-fw-split` | a chip profile (`SplitChip::Esp32s3`): GNU ld instead of lld, Xtensa relocation targets, literal pools |
| The records and the boot choice | `lp-bootctl` (`BootRecord` v1, the marks, `choose`, `EngineHeader` v1, `engine_digest`, the progress record) | — |
| The update session | `lpc-update`, unchanged | — |
| The core's side of an update | `fw-esp32-common`'s `split_core`, generic over a `SplitChip` | `fw-esp32s3/src/split_core/`, only what names the chip |
| The loader | — | `lp-fw/fw-esp32s3-loader`, the C6 loader's job on the S3's machine |

The S3's side of `split_core` covers its MMU, esp-storage's ROM flash calls,
its SHA accelerator, its reset table, its watchdog and its update light.

What the core holds without the engine, what core-only means, the
engine guard and the rules found on the C6's silicon carry over unchanged
from the C6's ADR (§§3–9 there).

### 2. Layout 2

A layout is exact offsets, compared for equality. Layout 1's region starts
at `factory + 0x8000`, which no 64 KiB page can hold, so the S3 gets
**layout 2** (`lp_bootctl::split_layout_2`):

| `factory +` | What |
|---|---|
| `0x0000` | the loader (at most `0x5000`) |
| `0x5000` | the update-progress record |
| `0x6000`, `0x7000` | boot records 0 and 1 |
| `0x8000..0x10000` | **reserved: erased, never written** |
| `0x10000` → `factory`'s end | the region (core and engine), every extent on a 64 KiB page |

Everything below `+0x8000` is layout 1's bytes at layout 1's offsets. So
the records, the marks, `choose`, the engine header, the progress record
and the engine digest are the same code on both chips. The 32 KiB gap is
the price of the page. Keeping it reserved, rather than giving it to the
loader, keeps the loader's limit one number. A layout code names offsets,
not a chip: a later chip with 64 KiB pages and this table may reuse 2.

### 3. The S3's MMU, and what the loader claims

The S3 has one 512-entry table, 64 KiB a page, serving the data bus
(`0x3C00_0000 + n·64 KiB`) and the instruction bus
(`0x4200_0000 + n·64 KiB`) at the same index. The plan is
`lp_bootctl::s3_mmu`:

| Entries | What |
|---|---|
| `0x000..0x100` | the core, from entry 0 = its first page |
| `0x100..0x1C0` | the engine, mapped once at `0x3D00_0000` (data, its header and rodata) and `0x4300_0000` (code) |
| `0x1C0..0x1E0` | the loader's scratch; after it jumps, the core's `lpfs` window (the tree store reads through the cache) |
| `0x1E0..0x200` | the core's own scratch (reading the records, hashing itself) |

The loader is never updated over the air, so what it assumes binds every
core it will ever boot:
- entry 0 is the core's first page;
- a core's flash segments stay below entry `0x1C0`;
- entries `0x1C0..0x200` are the loader's scratch, unmapped before the jump;
- the loader's RAM is `0x3FCC_5000..0x3FCC_9000` (data view, 16 KiB).

That RAM range is inside every core's `.bss`, at the top of its heap arena.
It was first `0x3FCB_0000..0x3FCC_0000` (64 KiB, mid-arena). It moved
before any update-capable S3 release, because the core cuts its heap arena
around the boot stages' RAM: with Bluetooth on by default the arena fell
into three pieces, and the largest free block fell from 213,868 to
73,744 B. At the arena's top it is cut once. It ends at `0x3FCC_9000`, where
esp-hal's `memory.x` lets a bootloader's startup IRAM begin. The engine's
entries and the `lpfs` window are per-build choices, not claims.

### 4. The codes

- **Chip 2** (`lpc_update::code_table::CHIP_ESP32S3`, word `"esp32s3"`) and
  **layout 2** (`LAYOUT_2`). The classic will get chip 3 when it comes.
- An S3 announces layout 2 in its manifest core and its hello. It does so
  only from the first core that can install an update; before that it
  announced 0, so Studio said "needs USB".
- A board refuses another chip's or another layout's offer by name before
  writing anything.
- A host meets these codes in two cases:
  - **A host that predates them** reads an S3 as another chip and decides
    "needs USB" (`lpa-update`'s decision, `OtherChip`). That is what it
    did before, so nothing in the field is surprised.
  - **A host that knows them** packages and offers an S3 build with the same
    `ota-manifest.json` format 1, core plus engine (`lp-cli`'s
    `requires_of`, chip-aware).

### 5. One door: the IDF bootloader

The S3 boots mask ROM → ESP-IDF's second stage (espflash's bundled
bootloader at `0x0`) → the loader (`factory + 0`) → core → engine. The C6
also boots through a second stage of our own. On the S3 that is future
work: the loader crate is shaped so that flavour adds a feature, a linker
script and a prologue without changing the loader's job. Until it exists,
every S3 suite runs one door where the C6's run two.

### 6. What the Xtensa link needed

GNU ld on Xtensa behaves in ways that lld on RV32 never showed. Each was
read off its output, never its source (AGENTS.md's licence rule), and each
is handled in `lp-fw-split` (its README lists them).

1. **Stale addends under `--emit-relocs`.**
   - A relocation against a merged section's symbol keeps the input
     section's addend, so `S + A` is wrong.
   - For an absolute word, the target is read from the linked bytes.
   - For an `l32r`, it is the word in the slot the instruction loads; a
     slot holding a plain constant is no edge.
   - For a narrowed `call8`, the target is decoded from the instruction.
   - For a **branch** (the radio blobs keep one on every jump), the target
     is decoded from the instruction too. Without that, the blobs' branches
     pointed at the literal pools at the start of `.text`, and those pools
     made the engine core: 1.3 MB with Bluetooth, 2.5 MB with Wi-Fi, where
     a core is a few hundred KB.
2. **Literal coalescing.** Equal literals of different functions share one
   pool slot, so code reaches its own pool one way only. A pool → code
   edge would make every function that shares a slot reachable.
3. **`l32r` reaches only 256 KiB backwards.** The engine script places
   each engine function's pool rule right before its code.
4. **The IDF bootloader aborts on an app with no flash segment.** The core
   image must still be a valid ESP image when the engine's sections are
   stripped from it. So the image carries placeholder flash segments. The
   padding segment espflash adds at address 0 is aimed at `.bss` instead.

The C6's output is byte-identical with the S3's profile in the tool. That
was checked by building it with and without each change.

### 7. The S3's chip layer

- **Hashing.** The core's two boot-path hashes (its own SHA-256, and the
  engine guard after a USB flash) run on the S3's SHA accelerator. A second
  caller that would interleave falls back to software. The tree store
  hashes its records in software.
- **Octal flash takes no update.** A part whose eFuse says octal flash
  forbids re-programming a word in place, and every boot-record mark is
  one. Such a board announces layout 0, writes nothing and runs its engine
  as a plain image.
- **The reset table** is the C6's classes, code by code, cited to the S3
  ROM's own names:
  - cold: power-on, brownout, deep-sleep wake, and the USB-Serial-JTAG
    and JTAG host resets;
  - warm: software resets, every watchdog, the super watchdog, an eFuse
    CRC error;
  - unknown is warm.

  So the boot choice means the same thing on both chips, and a host's
  reset never rolls a good build back (`lp_bootctl::ResetKind::from_s3_reason`).

### 8. The tree store on the S3, and no migration

The core that turns updates on also mounts the tree store on `lpfs`, as
the C6 does. `lpfs` keeps its 1.5 MB at `0x610000` (384 sectors), and
`factory` keeps its 6 MB. The core reads the store through its cache window
(§3), with the ROM's range invalidate after every program and erase.

A core that finds littlefs on `lpfs` holds it untouched (`fs: legacy_held`)
and never formats it. No S3 in the field holds user files (the only one is
the bench board), so there is **no migration**: the visit that turns
updates on is a reflash, and erasing `lpfs` over USB adopts the store
(`lp-fw/fw-esp32s3/README.md`).

### 9. Flash

On main at `06ea6b68a` (Bluetooth on by default, the update board side in;
`just fw-esp32s3-size-check`):

| | bytes |
|---|---:|
| loader | 3,744 |
| core | 717,680 |
| engine | 2,131,413 |
| `app.bin` (loader → engine end, core low) | 2,920,448 |
| monolithic image, for comparison | 2,788,640 |
| steady headroom (core low or high) | 3,373,611 |
| update headroom (a second core of the same size) | 4,784,128 |
| the core's distance to its next 64 KiB page | 3,216 |

The pre-merge gate holds the smaller of the steady and update headroom
above 64 KB. The `fw-esp32s3` README keeps the ledger.

## Consequences

- An S3 that has had its one USB visit takes every later build over the
  air, and, on the emulator, survives a power cut at any point with no
  cable (Evidence). Studio's update card, the release store and
  `lp-cli`'s update host take S3 builds as they take C6 builds.
- **The loader's MMU and RAM claims bind every future S3 core.** A core
  linked into entries `0x1C0..0x200`, or one whose loaded segments reach
  `0x3FCC_5000..0x3FCC_9000`, cannot boot through a loader already in the
  field.
- **Layout 2's offsets and codes are persisted** from the first
  update-capable S3 release. Changing them is a new layout and a new code,
  never an edit.
- Each S3 suite has one door until our own S3 second stage exists.
  Building that stage is the future work that adds the second door.
- On the S3, the core's `[CORE]` boot line rides the log channel and can be
  lost after an update's reset. That is open as
  `docs/defects/2026-10-22-the-s3-loses-its-core-line-after-an-updates-reset.md`.
  The S3's suites read the engine's hello instead.

## Alternatives Considered

| Option | Why not |
|---|---|
| Whole-image A/B in today's table | Did not fit: two 3.5 MB slots for a 2.45 MB image needed a repartition and a smaller `lpfs`. It also needed a one-image manifest and a second update target: a second Lasting format for the same job. The spike costed it at about the same agent-days as the split image |
| Repartition for A/B | Every S3 would have needed a migration, and `lpfs` would have shrunk to about 960 KB. No benefit the split image does not give |
| Our own second stage first, as the C6 now boots | Not needed to update, and it is the riskiest piece (the C6's took its own milestone). The IDF door works on every S3 now, and the second stage can be added later without changing the loader's job |
| Layout 1 on the S3 | Its region start (`+0x8000`) is not on a 64 KiB page; the S3's MMU has no smaller page |
| Hashing the boot path in software | The S3's SHA block is the C6's (the same driver code; the emulator models it bit for bit, since the IDF bootloader's image check needs it), so the C6's choice carries over; software stays as the fallback |

## Evidence

Emulated, `lp-emu:esp32s3:t1`, on the merged tree with the S3 emulator's
speed-up (`lp-emu` at `06ea6b68a`):
- **The split-boot scenarios** (`just test-emu-esp32s3-split-scenarios`):
  11 pass, 2 skip as the C6's do. They cover:
  - trial and confirm;
  - a warm death rolled back, naming the failed build;
  - cold retries to the cap;
  - a host reset counted cold (`rst:0x15`);
  - a started trial kept across power cycles;
  - a stale record cleared by a reflash;
  - an uncommitted engine not entered;
  - a broken core skipped;
  - a fallback read as a rollback;
  - a rollback that mounts the tree store.
- **The update scenarios** (`just test-emu-esp32s3-ota`): 13 pass, 1 skip
  as the C6's.
- **Power cuts during an X → Y**, 1,473 in all, none leaving a board that
  needs a cable (one X → Y is 26 s wall, one cut-and-recover 31 s):
  - after every one of the update's 697 requests;
  - torn (`lp-nor-sim`'s calibrated model) at every flash operation in the
    boot-record sectors (6) and the progress sector (704);
  - the suite's 66 sampled cuts, raw and compressed.

  A torn cut at every one of the update's 89,809 flash operations is
  queued as follow-up work.
- **The split boot** (`just test-emu-esp32s3-split-boot`, in CI): low core,
  high core, and a pushed project rendered.
- **The C6's suites**, unchanged by the shared board side: 13/13 and 14/14
  through both doors.

### Silicon — to be written after the desk sitting (2026-10-24 or later)

The board-side questions only silicon answers, from the desk sitting on
the bench S3 (XIAO ESP32-S3 Plus, 16 MB):
- MMU writes and cache invalidates while running from flash;
- fetches at MMU index `0x100` and above;
- the bootloader on the board being espflash's bundled one;
- the raw flash access the records and marks use;
- each reset's code against the table in §7;
- X → Y over USB;
- real power cuts.

The script is `scripts/ota/s3-desk-sitting.py`. **Nothing in this ADR has
yet been shown on silicon.**
