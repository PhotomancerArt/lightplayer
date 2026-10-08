# ADR: The ESP32-C6 ships as a split image — a loader, two boot records, a core and an engine inside `factory`

- **Status:** Accepted
- **Date:** 2026-10-04
- **Deciders:** Photomancer
- **Supersedes:** None (draft PR #903's design, which never merged)
- **Superseded by:** None

## Context

Updating firmware over the air needs a piece of the firmware that keeps
running while the rest is replaced: something that brings up the radios and
the links, accepts the new bytes, and can boot either the old build or the
new one. On the C6 that piece has to live inside the existing `factory`
partition. The partition table was just redrawn
(`2026-10-02-c6-repartition-and-layout-migration.md`), every board in the
field was migrated to it over USB, and a second redraw would cost every
board another migration.

A spike on the emulator and on two XIAO C6 boards (planning
`lp2025/2026-10-01-1854-ota-split-link-spike`, its `report.md`; draft PR
#903) showed that the firmware can be linked once and split by what the
boot path reaches: a **core** that boots, runs the radios, links and the
filesystem, and an **engine** holding everything else — the server, the node
graph and the on-device GLSL compiler — reached only through a header the
core reads at runtime. The core can then rewrite the engine (or a second
core) without rewriting itself.

This ADR records the layout, the formats and the rules that the product
image now ships with. Nothing in it transfers firmware over a link yet.

## Decision

### 1. One link, split by reachability

The firmware is linked twice. The first pass emits relocations and a map;
`tools/lp-fw-split` builds the section graph, walks it from the core's roots
(the reset entry, interrupt vectors, the core's statics) and writes a
linker script that places every section the core cannot reach into the
engine region (`0x4240_0000` in the flash window). The second pass links
with that script.

- **The verifier is a gate.** After the second pass the tool checks that no
  section the core reaches landed in the engine region. CI's
  `firmware-size` job fails on any; its report for this change reads
  `core nodes in engine region: 0; engine nodes left in core region: 26
  (4493 B)`.
- **The core reaches the engine only through data**: the engine's first
  bytes are its header (§5), and the core calls the entry point the header
  names. There is no direct call from core code into the engine.
- **The compiler is in the engine.** The verifier's report lists the
  engine's crates; for this change: `lpc_engine` 301,118 B, `lps_glsl`
  208,018 B, `lpvm_native` 66,420 B. Nothing is feature-gated out.
- **One tool owns the pipeline** (`tools/lp-fw-split`, a library and a
  binary): both passes, the verifier, the split into `core.bin` and
  `engine.bin` (with espflash 3.3's own image builder for the core), the
  loader build, `app.bin`, the whole-chip `merged.bin`, `split.json`, and a
  guard that fails the build if the source tree changes between the passes.
  `lp-cli firmware build|package` call the library; there is no Python and
  no binutils in the pipeline.

A plain `cargo build` of `fw-esp32c6` stays one link with the engine called
directly — the dev image that `just build-fw-esp32c6` and
`flash-fw-esp32c6` produce. What Studio flashes, CI gates and a release
ships is the split image.

### 2. The layout inside `factory`

```text
0x00000  IDF bootloader          espflash 3.3.0's bundled one, unchanged
0x08000  partition table          factory 0x10000+0x340000, lpfs 0x350000+0xB0000
0x10000  loader                   RAM-only, ≤ 0x5000 bytes; never updated over the air
0x15000  reserved                 an over-the-air update's progress record
0x16000  boot record, sector 0
0x17000  boot record, sector 1
0x18000  core                     an ESP image the loader starts
 page↑   engine                   its header first
  …      free to factory's end    read from the flashed partition table
```

- The fixed starts are offsets from `factory` (`+0x0`, `+0x5000`, `+0x6000`,
  `+0x7000`, `+0x8000`); `lp_bootctl::SplitLayout` is their one definition.
- **The region ends where `factory` ends in the table the chip carries.**
  The core reads the table once at boot; no constant names the end.
- A core alternates between the two ends of the region across updates: the
  first sits at `0x18000`, the next at the highest page-aligned offset that
  fits, the engine fills what is between.
- **The MMU page is 32 KiB.** `0x18000` is 32 KiB-aligned, not 64. espflash
  3.3.0's bootloader picks 32 KiB pages on a 4 MB C6; the packager aligns to
  32 KiB, the loader and the core read the page size from the MMU at
  runtime and refuse a layout that does not fit it, and the emulator's
  ROM-up boot gate runs that same bootloader, so a bootloader that picked
  64 KiB would fail CI.

### 3. The loader

`lp-fw/fw-esp32c6-loader` is the app the IDF bootloader starts. It is about
2.8 KB, runs entirely from RAM, and:

- reads both boot records, classifies the reset (§4) and chooses a core;
- maps that core with a scratch MMU window **bounded by the record's
  `core_len`**, copies its RAM segments and jumps to it;
- **falls back to the other record's core** when the chosen one does not
  load, printing why (`[LOADER] core @0x… skipped: …`);
- prints one ROM line naming the core and why it was chosen
  (`[LOADER] core @0x18000 (proven)`), the only output before the core runs;
- **never writes flash and never calls the ROM's SPI1 flash routines** (§7);
- carries a version word (`LPLV`, version 1) that the core finds by scanning
  the loader's first 4 KiB, and reports in its boot line;
- is never replaced over the air. Changing it takes a USB flash.

### 4. Boot record, version 1

Each record sector starts with 28 bytes written once into an erased sector,
then four marks the core programs later, each in place (1 → 0, no erase):

```text
 0  magic       u32   "LPBR"
 4  version     u16   1
 6  flags       u16   bit 0: trial (a core that has not proven itself)
 8  seq         u32   higher wins
12  core_off    u32   flash offset of the core's ESP image
16  core_len    u32   core.bin's exact length
20  build       u32   CRC-32 of the core's build id "<version>+<commit>"
24  crc         u32   CRC-32 of bytes 0..24
28  attempted   u32   the trial core first ran (before its radios came up)
32  confirmed   u32   the trial core's link came up
36  started     u32   the trial core finished its bring-up
40  cold_tally  u32   one bit cleared per counted cold retry
```

- **Torn writes err safe.** A torn record fails its magic or CRC and reads
  as no record. A torn `attempted`, `confirmed` or `started` reads as set
  (any cleared bit counts); a torn tally either counted or did not.
- **The choice** (`lp_bootctl::choose`): the newest valid record wins,
  unless it is a trial that **failed** and the other record is proven, in
  which case the loader rolls back. A failed trial with nothing proven
  behind it is booted anyway.
- **What "failed" means depends on the reset before this boot:**
  - *warm* (the chip reset itself — a panic, any watchdog): the trial ran
    and never confirmed;
  - *cold* (the power went, or a host reset the board): the trial died
    before it **started**, across **3** counted retries. A power cut in the
    first second of a new core is a retry, not a failure; a core that looks
    like a brownout is not retried for ever.
  - **A trial that started and has not confirmed is never failed by a cold
    boot.** Nobody has connected to it yet; power-cycling a board must not
    roll a good build back. A warm death after it started still fails it.
- **A host's reset is cold.** The USB-Serial-JTAG bridge's chip reset that a
  flasher or Studio drives (and a JTAG or SDIO host's) is not evidence that
  the core failed. The classification is one table, `lp_bootctl::ResetKind`,
  used by the loader and the core alike, cited to the reset-reason names in
  the C6 mask ROM's own table (the Technical Reference Manual's chapter was
  not available when it was written; the table says so). Unknown codes are
  warm.
- `build` lets a rolled-back core name the build that failed on it.

### 5. Engine header, version 1

The engine's first 88 bytes; the last word is the commit word, outside the CRC:

```text
 0  magic       u32   "LPEH"
 4  version     u16   1
 6  header_len  u16
 8  entry       u32   the engine's entry point
12  len         u32   the engine's length
16  build_id    64 B  "<version>+<commit>", zero-padded — must equal the core's
80  crc         u32   CRC-32 of bytes 0..80
84  commit      u32   exactly "LPOK", or the engine is not entered
```

- The packager patches `len` and `crc` after the second link (an entry
  pointer has no value to checksum at compile time) and writes the flashed
  image committed.
- **The commit discipline:** a writer writes the header sector with the
  commit word erased, then programs the word last. A torn header or a torn
  commit is never valid.
- The core maps exactly `len` bytes and enters only an engine of its own
  build, that fits its region and is committed; otherwise it runs
  **core-only** (§8) and says why (`no engine: engine not committed`).
- The build id is `<version>+<commit>`; `LP_BUILD_TAG` is gone. Two builds
  of one commit differ by their version (`APP_VERSION`).

### 6. Engine digest slot

The core carries a 40-byte tagged static (`LPED`, version, SHA-256) holding
the SHA-256 of `engine.bin` exactly as flashed (header patched and
committed). The split tool finds it by symbol in the second link and patches
it **before** the core's ESP image is made, so the image's checksum and
appended hash cover it; the packager refuses a core whose slot differs from
the engine it ships with. The core reads it volatile and names its first
four bytes in its boot line. **It is not checked at boot**: a core does not
hash its engine, and the loader does not hash a core. A future update
protocol verifies downloads against it.

### 7. Rules found on silicon

Each was found on XIAO C6 boards on 2026-10-02 while proving #903, and each
is now a rule of the code:

- **No ROM SPI1 flash access in the loader, and none in the core before
  `FlashStorage::new`.** esp-storage sizes the part with an `RDID` on SPI1;
  any earlier ROM flash access left that probe returning garbage, and every
  filesystem read then failed.
- **One `FlashStorage`.** A second one made while the radios ran also read a
  garbage size. The core's raw flash access for the records goes through one
  fenced handle.
- **The write fence.** Nothing is written outside the region, inside the
  running core, or before the fence knows both.
- **Trust the boot state only when it agrees with MMU entry 0** — the record
  the core believes it booted from must name the core the MMU actually maps
  — and only when every read succeeded and the table gave a layout.
  Otherwise nothing is written this boot, and the core says why.
- **Mark `attempted` right after `FlashStorage::new`**, before any radio
  comes up, so a core that dies in its bring-up is accountable.
- **A power cut is a retry, never a rollback** (§4).
- **An image ends on a flash sector, and a host write is checked**
  (2026-10-05, the bench C6 `A0:F2:62:87:B4:8C`, P10's first run). The
  packaged image ended at `0x2F55FE`; espflash 3.3.0's stub never wrote its
  last 254 bytes, and the core started the short engine, which faulted on
  every boot. The engine header's CRC covers the header only, so nothing at
  boot can notice (D20 keeps it that way). The host flasher now checks each
  write's MD5 before it resets the board.
  `docs/defects/2026-10-05-the-host-flasher-dropped-the-split-images-last-bytes.md`.

### 8. Core-only

When there is no engine it may enter, when its engine keeps crashing (four
incomplete boots, the recovery ledger's count), or while it is on trial, the
core runs core-only: its link up, the watchdog fed, nothing served. It sends
no hello, so a host reads it as a board with no usable firmware and offers a
USB update, which fixes it. A trial core confirms itself there once a host
is on its link.

### 9. Packaging

The flash unit stays **one merged image at `0x0`** — bootloader, table and
`app.bin` (loader, record 0 = sequence 1 proven, record 1 **erased**, core,
engine), up to `app.bin`'s end. `app.bin` ends on a **4 KiB flash
sector**, `0xFF` after the engine (`lp_fw_split::image_end`), and the
packager holds every image to that rule (added 2026-10-05: an image ending
mid-word lost its last 254 bytes to espflash 3.3.0's stub on silicon; see
§7). Studio's and the host's flashers write it
with no change, and their erase covers record 1, so a stale newer record on
a board (one that ran the spike) is gone after one Studio update.
`manifest.json` stays schema version 2 with an additive `split` block
(layout, page, build id, and offset, length and SHA-256 of the loader, core
and engine inside the merged image). `core.bin` and `engine.bin` are written
to `target/firmware-parts/<id>/`, never into the Studio bundle.

### 10. When version 1 binds

These formats bind from the **first release whose core can install an
update**. A core that ships before then cannot receive one, so a board on it
reaches that release only through another USB flash, which rewrites the
loader and the records. Until then a change to a format is an amendment to
this ADR, not a version bump. The torn-write discipline and the versioning
apply from now regardless.

## Consequences

- **Build time.** A split build is two fat-LTO links of the firmware (about
  50–100 s each on an M2 Max). CI's `firmware-size` job and the two emulator
  jobs each build one.
- **Size.** The split costs code (the boot bookkeeping, the header door and
  the loader: core + engine is 8,478 B more than the same tree's monolithic
  image) and, more, alignment: `app.bin` is 59,790 B larger than the
  monolithic image. Headroom now has two placements to satisfy; see the
  amendment to `2026-07-28-esp32c6-flash-budget.md`. Measured for this
  change: image headroom 371,202 B; steady headroom 371,202 B with the core
  low and with it high; update headroom (a second core of the same size
  beside it) 1,015,808 B.
- **The main stack.** The shipped image's main-task stack high-water on the
  emulator is 11,304 B, 2,608 B above the monolithic image of the same tree:
  the boot-record bookkeeping before the radios and the split boot's state
  holding the core's handles across the door.
- **Dev images stay monolithic.** `just build-fw-esp32c6`,
  `flash-fw-esp32c6` and the hardware-walk script flash the one-link image.
  A board flashed that way has no loader; an update must be refused there as
  "needs USB once".
- **Core-only is quiet** until a later milestone teaches it to name what it
  needs.
- **High-end cores occupy `0x310000–0x350000`**, which was the head of the
  filesystem before the repartition. Only migrated boards run split images,
  and the legacy guard keys on littlefs magic at the old offset, so a core
  there is never read as an old filesystem.
- **An update that writes a new core over the old engine must invalidate
  that engine first** (erase its header sector, or at least its commit
  word). The core enters an engine by its header alone, so an intact header
  over partly overwritten engine bytes would be entered. The emulator
  scenarios construct their interrupted-update states that way.
- **The emulated C6 performs no software reset yet**: a panic's reset
  reaches the board by the RTC watchdog. Rollback still works (that reset is
  warm), but the update protocol's resets need the emulator fixed first.

## Alternatives Considered

- **Fixed A/B app slots and `otadata`**, the ESP-IDF way: each slot holds a
  whole image, so neither fits beside the other in `factory`, and it needs
  a second repartition. Tried in the spike's first layout and dropped.
- **A second repartition** to make room: every board migrates again.
- **Placing crates, not reachable sections**: the spike found core code
  reached through generic instantiations in "engine" crates; only a
  reachability walk over the linked sections is correct.
- **Python tooling** (the spike's): it re-typed the record format by hand
  and needed binutils. One Rust crate uses `lp-bootctl` itself.
- **A hash of the core or the engine at every boot**: boot time and loader
  size, for a case the trial mechanism and a hash on download already
  cover.
- **Counting every cold boot of an unconfirmed trial toward the cap**: it
  rolls back good builds on boards nobody has reconnected to.
- **64 KiB alignment**: `0x18000` is not 64 KiB-aligned, and the pinned
  bootloader uses 32 KiB pages.

## Evidence

- Emulator (`lp-emu:esp32c6:t1`, lp-emu at the merge of this change's
  emulator work, `a2328ae3a`): the shipped split image boots from the reset
  vector through the ROM, the IDF bootloader, the loader and the core to the
  engine (`lp-emu-esp32c6/tests/split_boot.rs`, in CI); a direct load of the
  loader over the flashed chip agrees with that boot on the MMU table, every
  byte of the second link and the console; the core's boot line and the
  engine's hello over the link (`lp-cli/tests/emu_split_boot.rs`, in CI);
  the emulator walk renders byte-identically on all three readings; Studio
  flashes the packaged image onto a blank emulated board and it boots
  (`just walk-no-board`).
- Scenarios (`just test-emu-c6-split-boot`, not CI): trial and confirm;
  warm rollback naming the failed build; cold retries to the cap, then
  rollback; a host reset counted cold; a started trial surviving power
  cycles; a stale newer record cleared by reflashing the packaged image; an
  uncommitted engine not entered; a broken core skipped for the other.
- Silicon: see this change's pull request (the hub board's flash and power
  cycles).

**Provenance:** OTA roadmap `lp2025/2026-10-03-1330-ota-firmware-updates`
(milestone M2), plan `lp2025/2026-10-04-0005-ota-split-image-ships`
(decisions D1–D21), spike `lp2025/2026-10-01-1854-ota-split-link-spike`,
draft PR #903.

## Amendment (2026-10-07): a trial core that hears from no host gives the board back

§4 keeps a trial that started and never confirmed alive across every cold
boot: nobody has connected to it yet. Over Wi-Fi that waits for ever on a
house board out of Bluetooth range whose new core cannot reach its network.
So (OTA Wi-Fi plan WD9; W4 answered yes, three minutes, DD67):

- **A trial core whose boot read a saved network with Wi-Fi on**, and on
  which no host link has come up on any transport for **three minutes of
  its own uptime**, logs `[OTA] trial: no host in 3 min — giving the board
  back to its last good core` and resets itself with a software reset.
- That reset is **warm**, and a warm death after `started` fails a trial
  (§4's table, `lp_bootctl::choose`): the loader rolls back to the proven
  record. **No new mark, no record change, no loader change.**
- The old core comes up engine-less (the update erased the engine header
  before the core moved), says so on Wi-Fi, and any host holding its engine
  heals it. The board refuses the failed build from then on
  (`refusedBuild`).
- It never applies to a boot with no network saved or Wi-Fi off (Bluetooth
  or USB is how that board's owner reaches it), nor once a host link has
  confirmed the trial.
- **It is not a counted cold retry.** The cold rule is about a core that
  dies while bringing its radios up; this one started and ran, and simply
  heard nobody. An unattended update abandoned for three minutes rolls back
  rather than waiting: the price of never needing a cable.

The rule is `lpc_update::board::TrialDeadline` (`TRIAL_HOST_DEADLINE_MS`),
host-tested there; the emulated proof is `lp-cli/tests/emu_ota_lan.rs`'s
L8 (the host gone at the core's commit; the trial rolls back, refuses Y,
and heals over the LAN).
