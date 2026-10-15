---
status: open
found: 2026-10-14      # hardware smoke: RAM research V3 (#1110 merged with main) on c6-expendable (CX1)
area: fw-esp32c6-loader esp_image::load × lp-fw-split's core image (espflash 3.3.0's image builder) × the C6 core's rodata/text page boundary
class: config-masked-defect   # the loader is correct only while the core's RAM segments are big enough to fill the gap before flash .text; #1092 shrank them
related:
  - 2026-10-04-c6-split-link-firmware-loader-and-boot-records   # ADR: the loader is never updated over the air
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

**Not done here.** No fix. Options for whoever takes it:

- `lp-fw-split` refuses a core with an address-0 segment (a build-time check,
  so the image never ships);
- the image builder points the leftover at RAM the core zeroes anyway;
- the layout keeps flash `.text` from needing a gap.

#1110 is held in draft until one of them lands.
