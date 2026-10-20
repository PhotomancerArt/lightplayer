---
status: fixed          # guarded by #1143 (no such core builds or ships); fixed by #1144 (the padding goes to .bss)
found: 2026-10-14      # hardware smoke: RAM research V3 (#1110 merged with main) on c6-expendable (CX1)
fixed: this change     # #1144
area: fw-esp32c6-loader esp_image::load × lp-fw-split's core image (espflash 3.3.0's image builder) × the C6 core's rodata/text page boundary
class: config-masked-defect   # the loader is correct only while the core's RAM segments are big enough to fill the gap before flash .text; #1092 shrank them
related:
  - 2026-10-04-c6-split-link-firmware-loader-and-boot-records   # ADR: the loader is never updated over the air
  - docs/defects/2026-10-14-the-emulated-c6-ignores-stores-to-address-0.md
  - lp2025/2026-10-09-1203-ram-research   # V3's report and evidence/step1/
---
# The C6 split loader copies an address-0 padding segment to address 0, and the core never starts

**Symptom.** I merged #1110 (C6 match tables out of RAM) with main, built the
split image (`just fw-esp32c6-split`, `esp32c6,server`) and wrote its app range
to CX1 (`c6-expendable`, 14:C1:9F:E6:54:90). Every byte was MD5-verified on the
chip first. The image faults in the loader on every boot, before the core
starts:

```
I (140) boot: Disabling RNG early entropy source...
Guru Meditation Error: Core 0 panic'ed (Store/AMO access fault)
PC      : 0x408500ca ... A4 : 0x00006754 ... MCAUSE : 0x00000007
```

**Cause.** The core image (`core.bin`) has an address-0 segment:

| core image | rodata segment ends | flash `.text` at | gap before `.text` |
|---|---|---|---|
| release 2026.10.14-10 (`f7fc2c781`) | `0x4202ff18` | `0x42030020` | 248 B, filled with RAM segments |
| #1110 + main (`5fb7d260b`) | `0x42030080` | `0x42040020` | 26,452 B left over → `addr 0x00000000 len 26452` |

espflash 3.3.0's image builder (`image_format.rs`, `get_segment_padding`) fills
the gap before a flash segment with the image's RAM segments first, and writes
an address-0 segment for whatever is left. The IDF bootloader skips a segment
like that. `fw-esp32c6-loader/src/esp_image.rs` treats every segment outside
the MMU window as RAM and copies it with `flash.copy(data, addr as *mut u8,
len)`. Here that copy writes to address 0, and the store faults.

Two things have to happen together:

- **rodata crosses a 64 KiB page.** Then flash `.text` starts one page later,
  and the gap before it is nearly a whole page. #1110 moved 360 B of lookup
  tables to rodata. Main's rodata ends **232 B** below `0x42030000`.
- **Too little RAM to fill the gap.** Before #1092 the radio blobs' IRAM
  (~66 KB) filled it whatever its size. Since #1092 the core's RAM segments
  come to about 45 KB on main (about 39 KB with #1110), so a whole-page gap
  leaves an address-0 segment behind.

So a change to main that adds about 232 B of core rodata, from any source,
makes a core that will not boot on silicon.

**Why it matters more than one PR.** The core marks a trial `attempted`
itself (`fw-esp32c6/src/ota/boot_state.rs`), and `lp_bootctl::failed` will not
roll back a trial that was never marked attempted. A core like this pushed
over the air faults in the loader before it can set that mark. Reading the
code (not tested), the board then boots the faulting trial over and over until
someone connects a cable. That breaks the OTA invariant. The loader is never
updated over the air, so the fix has to be in the image: no core may carry an
address-0 segment, or any segment whose address is not memory the loader may
write. A loader change helps only boards flashed afterwards.

**The emulator does not catch it.** Under `lp-cli emu run --merged` the same
image logs `UNMAPPED write1 at 0x00000000 from pc=0x408500ca` 26,452 times,
ignores each write, and boots the core (`[LOADER] core @0x18000 (proven)`).
That PC is the one that faulted on silicon. A green emulator job on such an
image says nothing about whether it boots (`--strict-bus` would fault).

**The guard (#1143).** `lp-fw-split` now lists `core.bin`'s segments and
refuses the build when the loader cannot place one
(`tools/lp-fw-split/src/core_segments.rs`). A segment must lie inside the
core's flash (`0x4200_0000` up to the engine region, at its page offset) or
inside the core's RAM (esp-hal's C6 `RAM` region, `0x4080_0000..0x4086_E610`)
less the loader's own (`LOADER_RAM`, `0x4085_0000..0x4086_0000`). The whole
class is refused, not only address 0. `lp-cli firmware package` checks the
same thing on the bytes it ships. So `just fw-esp32c6-split`,
`just fw-esp32c6-size-check` (pre-merge CI) and
`scripts/release/release-firmware.sh` all fail on #1110 + main, and pass on
main. A test pins those ranges to the loader's source. A scan of every
release since #1092 (2026.10.10-24 through 2026.10.14-13) found no shipped
core with such a segment. `lp-fw-split segments <core.bin>` lists any image's
segments.

The emulator now faults a store where the C6 has nothing, as silicon does
(`docs/defects/2026-10-14-the-emulated-c6-ignores-stores-to-address-0.md`).

**The fix (#1144).** `lp-fw-split` gives each padding segment a **load
address in the core's `.bss`**: a run from `_bss_start`, read from the
pass-2 ELF (`tools/lp-fw-split/src/core_padding.rs`). The segment's bytes stay
where they are, so every later segment keeps its offset and its page
alignment. The loader then copies zeros into `.bss`, which riscv-rt zeroes
before Rust runs. Nothing is kept there across a reset (`.noinit` and
`.rtc_fast.persistent` sit outside it). A run that would leave `.bss`, reach
the loader's RAM, or overlap another segment is refused, and the guard still
checks the result. The image's appended SHA-256 is recomputed. Nothing changes
in the loader, the boot records or any format, so boards already fielded with
today's loader boot the new cores. A core whose rodata does not cross a page
has no padding, and its image is unchanged.

The other two options were rejected. Refusing the core only moves the
failure into whichever PR next adds ~224 B of core rodata. A layout that never
needs a gap would mean changing esp-hal's `.text_gap` or espflash's 64 KiB
alignment. Both are wider changes than this, and both depend on how much RAM
there is.

Proof, #1110 + main with the fix (core 1,443,952 B, padding 26,452 B at
`0x40809848`), on CX1 (`c6-expendable`, 14:C1:9F:E6:54:90):

- over USB: `[LOADER] core @0x18000 (proven)`, then the engine and the
  project, at 33–35 fps;
- over the air on USB from release 2026.10.14-13: the new core at the high
  offset `0x1e8000`, on trial, then proven, the engine healed,
  `done: UpToDate (32.4 s)`;
- real power cuts (`board power-cycle`) mid-core and mid-engine: both
  resumed (`resuming core at …`, `resuming engine at 1609728`) and ended on
  the new core, proven, with its engine.

Emulated (`lp-emu:esp32c6:t1`, at this change's tree): `split_boot.rs` on the
crossing image (ROM-up to the engine, strict bus), and a cut sweep from main
to the crossing image (4 of 4). `just test-emu-c6-split-boot` and
`just test-emu-c6-ota` pass on main's images.

**Regression coverage.** `core_padding` unit tests (#1110 + main's segment
table, two paddings, every refusal); `core_segments`, the guard, on every
build; the emulator's `a_core_segment_at_address_0_faults_the_loader_as_on_silicon`.

**Lesson.** A format the bootloader tolerates by convention (skip an
address-0 segment) is not one our own reader tolerates, unless something
holds the image to what that reader accepts. Here it was "page-congruent,
and RAM or flash". The loader cannot change after it ships, so that check
belongs in the build, and the image is where any fix has to happen.

**Later (2026-10-15): the gap itself is gone.** The "layout that never needs
a gap" rejected above turned out to need neither esp-hal nor espflash to
change for every image: `lp-fw-split` links the core with esp-hal's
`.text_gap` switched off (the esp-hal fork's sixth diff), because the split
loader maps every page a segment touches and the IDF bootloader never loads
the core. A crossing core is now one flash segment with no padding, and the
~20 KB it cost is back (RAM research L2;
`docs/adr/2026-07-28-esp32c6-flash-budget.md`, 2026-10-15 amendment). The
`.bss` re-aim stays as the second line behind it.
