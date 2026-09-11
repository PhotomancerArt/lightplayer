# `lp-emu-esp32s3` — the ESP32-S3 machine

This is where the S3's chip numbers will live. `lp-emu-esp-common` supplies
the bus, the peripheral model, the trace and the ELF view and knows nothing
about any chip; `lp-xt-emu` supplies the Xtensa hart and knows nothing about
MMIO. There they are put together with a memory map, a mask ROM, a reset state
and a run loop, and the result takes a `fw-esp32s3` binary.

It is `lp-emu-esp32v3`'s twin — same hart, same shape, same module names — and
`lp-emu-esp32c6`'s where the peripherals are concerned.

> **M6 P01. There is no machine here yet, and that is the whole point of this
> phase.**
>
> **What exists:** the generated register-name tables for twenty-two blocks
> ([`src/regs/`](src/regs/)), a vendored `esp32s3_rev0_rom.elf`
> ([`../roms/`](../roms/)), and a static inventory of what the shipped image
> actually does —
> [`docs/reports/2026-09-11-esp32s3-firmware-inventory.md`](../../../docs/reports/2026-09-11-esp32s3-firmware-inventory.md).
>
> **What does not exist:** a memory map, a bus, a hart, a peripheral, a mask
> ROM loader, a direct load, a run loop, a snapshot, a CLI, a binary, a
> `just emu-esp32s3` door, and any test that runs a guest instruction. P01
> measured; P02 answers the bus-alias question, P03 builds the machine and the
> map, P04 the blocks, P05 the link and the hello, P06 the flash cache and the
> ROM-up path, P07 the pads and the frame.
>
> `Cargo.toml` already declares `lp-emu-core`, `lp-emu-esp-common`,
> `lp-xt-emu` and `lp-ws281x` even though nothing here imports them. That is
> deliberate, and it is what the classic's manifest says about its own first
> phase: **the dependency edge is what makes P03's `machine.rs` an addition
> rather than a manifest change nobody reviewed.**

## What the inventory found, in one screen

The numbers and their commands are in the report; this is what a reader who is
about to write a phase needs to know before opening it.

| | |
|---|---|
| **Decoder coverage** | 99.53 % of 725,373 instructions, **0 mismatches**. The classic, same HEAD: 98.86 % with 2 |
| **PIE / `ee.*`** | **A phantom.** 3,354 apparent `ee.*` sites, **0 of them inside a sized code symbol** — interleaved literal-pool bytes `objdump` mis-decodes as vector math. The plan's "the `ee.*` extension is out of scope" is statically justified, not assumed |
| **The alias** | Statically no executable section is placed through the D-bus view; dynamically the product JIT path writes a shader through D and fetches it through I. See below |
| **Blocks touched** | 22, of which **eight are not in the milestone brief's list** — `SPI0`, `SPI1`, `APB_CTRL`, `I2C_ANA_MST`, `BB`, `NRX`, `FE`, `FE2`, all on `esp_hal::init`'s inlined path |
| **UART** | **Not touched at all.** No UART0 register, no ROM UART routine. The S3 is the first machine in this plan with no UART on the application path; its console is `esp-println`'s `jtag-serial` over USB-Serial-JTAG |
| **The ROM** | 45 distinct entry points, and `memcpy` alone is **4,769 call sites across 800 caller symbols**. On this chip the ROM is most of the dynamic instruction count, not a formality |

### The alias answer, which D2 rests on

> **The shipped S3 image does not write bytes through one view of SRAM1 and
> fetch them through the other at link time — and does exactly that at run
> time, on the product path, for every shader it compiles.**

A machine that maps only the ELF's sections boots this firmware perfectly and
then faults on the first shader. The report's §5 has both halves with their
evidence.

## Three things that will bite a phase that assumes the C6's or the classic's

1. **`EXTMEM`'s cache-enable polarity is inverted relative to the C6's.** The
   S3's `icache_ctrl.icache_enable` bit 0 is "0 disable, 1 enable"; the C6's
   `l1_icache_ctrl.l1_icache_shut_ibus0` bit 0 is "0 enable, 1 disable". A
   cache-off watch copied from the C6 arms backwards.
2. **The flash-MMU table is not in any register block.** Not in `EXTMEM`, not
   in `SPI0` (the S3 has no `mmu_item_index`/`mmu_item_content`), not in
   `esp-metadata-generated`. The report's §8 reads it out of the vendored
   ROM's own `Cache_*` disassembly, with the ROM address beside every number.
   Nothing here may be taken from the classic's DPORT tables.
3. **`interrupt_core0` and `interrupt_core1` are one 4 KB window**, core 0 at
   `+0x000` and core 1 at `+0x800`, which is why the generated
   `INTERRUPT_CORE1` table's first entry is at `+0x800`. The PAC gives both
   types the same base and that is correct, not an SVD leak.

## The register tables

Generated from the `esp32s3` PAC's svd2rust offset comments by
`scripts/emu/pac-regnames.py --pac esp32s3`, with the provenance header
`docs/adr/2026-07-29-license-provenance-discipline.md` requires, and checked by
`just lint-emu-regnames` — which checks all three chips, so a hand edit here
fails the same lint a hand edit in the C6's tables does. **Never hand-edit a
file under `src/regs/` except `mod.rs`**, which is hand-written and carries the
prose and the assertions no generator could produce.

Twenty-two blocks: every one the image's own MMIO census names, plus `uart0`
and `sha` for the ROM-up path. A table nothing reads is cheap; a missing one is
a phase blocked on a regenerate.

⚠️ **One table is deliberately absent: the interrupt-source numbers.**
`esp32s3-0.35.2` puts its `Interrupt` enum in `src/lib.rs`, not in a
`src/interrupt.rs` as the C6 does, and the generator reads the latter by name.
Teaching it a second path is a generator change and P01's scope was the chip
entry, so the phase that first needs the table makes it. The four numbers a
phase needs in the meantime are in M6 notes §3.3: `RMT = 40`,
`TG0_T0_LEVEL = 50`, `SYSTIMER_TARGET0..2 = 57,58,59`, `USB_DEVICE = 96`.

## Licence

MIT, as a unit with the rest of `lp-emu/` — not the workspace's AGPL. See
`lp-emu/LICENSE-MIT`, `docs/adr/2026-09-06-lp-emu-home-and-mit-fence.md` and
`just lint-emu-fence`.
