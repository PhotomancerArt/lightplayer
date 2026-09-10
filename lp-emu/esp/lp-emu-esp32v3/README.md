# `lp-emu-esp32v3` — the classic ESP32 (v3) machine

This is where the classic's chip numbers live. `lp-emu-esp-common` supplies
the bus, the peripheral model, the trace and the ELF view and knows nothing
about any chip; `lp-xt-emu` supplies the Xtensa hart and knows nothing about
MMIO. Here they are put together with a memory map, a mask ROM, a reset state
and a run loop, and the result takes a `fw-esp32v3` binary.

It is `lp-emu-esp32c6`'s twin, deliberately: same shape, same module names,
different silicon.

> **M3 P1.** What exists today is the memory map, the generated register-name
> tables and the vendored mask ROM. The sections below that name a later phase
> are stubs, and they say so rather than describing a machine that does not
> exist yet.

## The machine

*P2.* One bus, two `XtHart` slots with core 1 stalled, one schedule.

## The memory map

`memmap.rs` holds every base and length, each cited to
`third_party/esp-hal/ld/esp32/memory.x`, the hardware-measured board profile
in `lp-emu/lp-xt-emu/src/board.rs`, the vendored ROM ELF's own program headers
or `esp32-0.40.2`. Nothing else in the crate writes an address literal.

```text
0x3F40_0000 +0x40_0000  DROM window   flash .rodata (cache MMU)  R
0x3FF0_0000 +0x08_0000  MMIO          the peripheral window
0x3FF8_0000 +0x0_2000   RTC_FAST (D)  8 KiB
0x3FF9_6000 +0x0_942A   ROM data      mask ROM .rodata           R
0x3FFA_E000 +0x0_2000   SRAM2 (ROM)   the 8 KiB memory.x reserves
0x3FFB_0000 +0x3_0000   SRAM2 dram_seg  192 KiB app data         RW
0x3FFE_0000 +0x2_0000   SRAM1 (D-bus) ROM data + both ROM stacks RW
0x4000_0000 +0x6_5D90   mask ROM      code + the vector table    RX
0x4007_0000 +0x3_0000   SRAM0         cache seg + vectors + IRAM RX (word-only)
0x400C_0000 +0x0_2000   RTC_FAST (I)  same block as 0x3FF8_0000  RWX
0x400D_0000 +0x30_0000  IROM window   flash .text (cache MMU)    RX
0x5000_0000 +0x0_2000   RTC_SLOW      8 KiB                      RW
```

Three splits are decisions rather than transcription, and each is argued in
`memmap.rs`'s own module docs:

**The SRAM1 I-bus alias (`0x400A_0000..0x400C_0000`) is deliberately
unmapped.** The classic mirrors SRAM1 into the instruction bus, and M0's
inventory measured **zero** in-symbol-bounds references to that window in the
shipped `fw-esp32v3` image and in the classic mask ROM
(`docs/reports/2026-09-10-xtensa-firmware-isa-inventory.md` §5). It is named
anyway — `SRAM1_IBUS_ALIAS_BASE`, with no region behind it — so a strict-bus
stop can say "the SRAM1 I-bus alias, which this machine deliberately does not
map" instead of "unmapped". Director ruling DD24.

**SRAM0 is one executable region from `0x4007_0000`.** The app's linker script
calls the first 64 KiB `reserved_cache_seg`, because with the cache on it *is*
the cache array — but the ESP-IDF second-stage bootloader executes from
`0x4007_8000`, inside it (`esptool image-info` reads bootloader segment 1 as
`0x4007_8000`, 15,576 bytes; the desk board's ROM banner prints
`load:0x40078000,len:15576`). One memory, two lives, one region. The honest
diagnosis for a *cache-off* access through the flash windows is D4's stop
(P4), which names the DPORT bit and the cycle the cache went away — not an
unmapped hole that says nothing about why. Director ruling DD24.

**The SRAM0 word-only rule is a guest rule, and it is measured.**
`lp-xt-emu/src/board.rs:156-172` records the experiment: on the desk board a
byte store at `0x4008_8000` faulted with `LoadStoreError` / EXCCAUSE 3 /
EXCVADDR = that address, while 16,384 aligned word stores across
`0x4008_8000..0x4009_8000` all read back. Guest accesses are held to that.
Host-side placement — seeding the ROM, placing an ELF segment, filling a cache
line — writes bytes, because that is the emulator putting memory into the
state silicon was handed, not the guest reaching through the instruction bus.

Two more addresses are named here and nowhere else: the flash MMU page tables
at `0x3FF1_0000` (PRO) and `0x3FF1_2000` (APP). They are raw 256-entry `u32`
arrays inside the DPORT window but past the end of the register block svd2rust
generates, so the generator cannot name them; P4 declares their format from
the ROM's own `Cache_Flash_MMU_Set`, never from a datasheet.

## The ROM is loaded in every configuration

Plan PD7, vision D6. The application calls into the mask ROM at runtime
whatever booted it: `rtc_get_reset_reason` from `__pre_init`, `ets_delay_us`
from every clock path, `uart_tx_one_char` from esp-println,
`esp_rom_spiflash_*` from esp-storage. So the ROM image is part of the memory
map, not an extra for a ROM-up boot.

The image is `esp32_rev300_rom.elf`, vendored at `../roms/` with its licence
and checksums. It is `EM_XTENSA`, `e_type = EXEC`, entry `0x4000_0400` — the
same `XCHAL_RESET_VECTOR_VADDR` that `xtensa-lx-rt`'s `config/esp32.rs`
declares, so two independent sources agree on where a reset lands.

**Which revision, and why only one.** The release tarball carries `rev0` and
`rev300`; this crate vendors **`rev300` only** (plan decision Q2). The desk
board and every board this firmware ships on are v3 silicon, and a second
826 KB ELF that no configuration loads is 826 KB of repository nobody can
check. Adding one later is one line in the fetch script's `WANTED` array.

**Nothing here is edited, ever.** `scripts/emu/fetch-rom-elfs.sh` fetches the
published tarball, verifies its pinned sha256 and byte count, extracts the
wanted files and re-derives `SHA256SUMS` from the extracted bytes.
`--check` verifies the vendored files offline, and
`tests/rom_vendoring.rs` re-derives the digest **in-process** from the same
bytes the machine will load — so a corrupted or swapped ROM fails the build's
tests rather than a boot at cycle 400,000. Editing `SHA256SUMS` to make a
check pass turns a checksum into a decoration; re-run the script instead.

The embedded copy spells its length out in its type
(`&Aligned<[u8; ROM_BYTES]>`), so a file of a different size fails to
**compile**, and it is wrapped in an alignment shim because `include_bytes!`
promises nothing about alignment and `object`'s ELF reader casts into the
buffer (the same shim, for the same reason, as
`lp-emu-esp32c6/src/rom.rs:46-74`).

## Direct load

*P3.* `--elf`, and the list of everything the real ROM-and-bootloader path
does that this one does not.

## Booting from the reset vector

*P7.* The mask ROM boots the espflash-merged image through the real IDF
bootloader, and the boot log is compared line for line against silicon.

## The CH340 cable

*P6.* On the classic the port is a **bridge chip on the board**, not a
peripheral inside the SoC, so opening the port moves no chip state: what
resets the chip is the auto-reset circuit driven by the modem lines, and the
truth table is the board's, not the chip's.

## Flash, and the cache window

*P7.* SPI0/SPI1 on `engine::spi_flash`, and the classic cache-MMU fill.

## Peripherals

*P4–P8.* The grade table, one row per block, as the C6's README carries.

## The CLI

*P2 skeleton, P8 completion.* `just emu-esp32v3 <elf>` is the door; the recipe
exists from P1 so the door has one name for its whole life.

## Bring-up loop

*P3.* Run strict, read the first stop, model that block, run again.

## Tests

| Test | What it holds |
|---|---|
| `tests/memmap.rs` | No two declared regions overlap; every `periph::*` base is inside the MMIO window; `dram_seg` is 8 KiB above the ROM's reserve with `RESERVE_DRAM = 0`; the vector table is 1 KiB below the IRAM; SRAM0 is one region containing the bootloader's `0x4007_8000`; the SRAM1 I-bus alias is named and unmapped; the ROM extents match the vendored ELF; 240 cycles to the microsecond |
| `tests/rom_vendoring.rs` | The embedded ROM's sha256, re-derived in-process, is the one `SHA256SUMS` records — and the file on disk is still that file |

Run them with `just test-emu-esp32v3`.

## Provenance

- The mask ROM is Apache-2.0, from `espressif/esp-rom-elfs` release
  `20260528`. It is committed **verbatim**; see `../roms/README.md`.
- The register-name tables in `src/regs/` are **generated** from the `esp32`
  PAC's svd2rust offset comments by `scripts/emu/pac-regnames.py --pac esp32`
  and carry the provenance header
  `docs/adr/2026-07-29-license-provenance-discipline.md` requires. Never
  hand-edit one; `just lint-emu-regnames` catches it, for both chips.
- `RNG` is **not** generated. `esp32-0.40.2/src/lib.rs:647` gives it base
  `0x6003_5000`, an address that does not exist on this part — an SVD leak
  from the S2/C3 family. It is in the generator's `SKIP` table with that
  reason; P7 resolves the classic's `WDEV_RND_REG` from the ROM ELF's own
  symbol or from esp-hal's classic `rng`.
- Every constant in `memmap.rs` carries the `file:line` it was read from.
- The crate is MIT, as a unit with the rest of `lp-emu/`. See
  `../../README.md` and `just lint-emu-fence`.
