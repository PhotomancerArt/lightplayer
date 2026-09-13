# `lp-emu-esp32s3` — the ESP32-S3 machine

This is where the S3's chip numbers live. `lp-emu-esp-common` supplies the
bus, the peripheral model, the trace and the ELF view and knows nothing about
any chip; `lp-xt-emu` supplies the Xtensa hart and knows nothing about MMIO.
Here they are put together with a memory map, a mask ROM, a reset state and a
run loop, and the result takes a `fw-esp32s3` binary.

It is `lp-emu-esp32v3`'s twin — same hart, same shape, same module names — and
`lp-emu-esp32c6`'s where the peripherals are concerned.

> **M6 P03. There is a machine here now, and it has no peripherals — which
> is the point.**
>
> **What exists:** the memory map ([`src/memmap.rs`](src/memmap.rs)), the bus
> it builds ([`src/bus_setup.rs`](src/bus_setup.rs)) **with SRAM1's I-bus view
> as a RAM alias**, the mask-ROM loader ([`src/rom.rs`](src/rom.rs)), the
> direct load ([`src/loader.rs`](src/loader.rs)), two hart slots and the
> quantum run loop ([`src/machine.rs`](src/machine.rs)), the snapshot, the
> generated register tables, and a binary — `just emu-esp32s3 <elf>`.
>
> **What does not exist:** any peripheral. With the MMIO window declared and
> nothing inside it, a `--strict-bus` run stops at the **first** block the
> boot touches and says which; that stop is P03's deliverable and P04's first
> ledger entry. There is also no console (P05), no flash cache and no ROM-up
> boot (P06), and no pad fabric (P07).

## Running it

```bash
just build-fw-esp32s3
just emu-esp32s3 target/xtensa-esp32s3-none-elf/release-esp32s3/fw-esp32s3 --strict-bus
just test-emu-esp32s3          # the suite; builds no firmware
just test-emu-esp32s3-boot     # …plus the image-backed half
cargo run -p lp-emu-esp32s3 --release -- --map
```

An unrecognised flag is an **error**, not a no-op: a door a later phase adds
must be visible in that phase's diff. `--help` lists what exists.

## The bring-up loop, and where it stops today

1. Run `--strict-bus`.
2. Read the **first** stop. The earliest strict stop is the root; an exception
   after it is downstream and tells you nothing.
3. Model that one block, with the pin cited — a PAC reset value, a ROM
   disassembly, a linker-script constant. Never "what the boot needed".
4. Run again.

P03's own run of the shipped image stops here:

```text
STRICT BUS STOP
  pc      = 0x4004f670 (Cache_Occupy_ICache_MEMORY+0xc)
  cycle   = 36 (0 us emulated)
  access  = Read Word at 0x600c1004
  where   = inside the declared peripheral window — an UNMODELLED BLOCK
```

⚠️ **That is `SENSITIVE + 0x04` (`cache_dataarray_connect_1`), and the MMIO
census says `sensitive` is not touched.** Both are true: the census
(`m6/notes.md` §2.4) swept the *application's* `l32r` literals, and this
access is the **mask ROM's**, on `esp_hal::init`'s
`rom_config_instruction_cache_mode` path. It is also the evidence that the ROM
really executes — which on this chip is not a formality (§2.5: `memcpy` alone
is 4,769 call sites), and `tests/boot.rs` calls ROM `memcpy` over guest memory
to say so directly.

## Three things about this machine that its siblings do not have

1. **SRAM1 is one region with two doors.** `0x3FC8_8000` on the data bus and
   `0x4037_8000` on the instruction bus, `0x6F_0000` apart, one store. The
   classic and the C6 name their aliases and leave them unmapped (DD24/DD36)
   because nothing reached them; here the product path writes every JIT'd
   shader through the D-bus view and fetches it through the I-bus one, so a
   machine that mapped only the ELF's sections would boot perfectly and fault
   on the first shader. `SocBus::add_ram_alias` (M6 P02) is what makes it one
   store, and because translation happens *before* the region lookup (DD81)
   it is the **D-bus region** that carries the executable flag.
2. **Slot 1 is held by the chip, not just by the machine.**
   `SYSTEM.core_1_control_0`'s PAC reset value is `0x04` — `reseting` set,
   `clkgate_en` clear — so core 1 is held before any software runs. The hold
   is modelled with the register cited; the **release is not implemented**,
   because the classic's model of where a released core starts (DD53) is a
   classic-silicon finding about different registers and there is no S3
   measurement.
3. **`CPENABLE`'s reset is a parameter, not `0xff`.** The classic's `0xff` is
   measured on classic silicon, and `lp-fw/fw-esp32s3/src/board/esp32s3/fpu.rs`
   records the S3 board's own `0xff` reading as "a measured fact about *this
   boot chain*, not a guarantee from the architecture". So the default is the
   ISA's generic reset and `--cpenable-reset` is how P09's capture changes it.

## What the ISA-gap test found

`tests/isa_gaps.rs` is the first thing P03 ran, before a map existed: every
S3-only mnemonic and special register `m6/notes.md` §2.3 measured, assembled
with `lp_xt_inst::encode` and run on a bare `XtHart`.

| what | sites | result |
|---|---:|---|
| `s32c1i` + `wsr.scompare1` | 176 | arm present; both outcomes asserted |
| `wsr.atomctl` | 1 | arm present (accept-and-remember, and said to be) |
| `wsr.intset` | 6 | arm present; software lines only, per the RM |
| `esync` | 1 | arm present |
| `wdtlb` / `witlb` | 1 each | arm present; read back through `rdtlb1`/`ritlb1` |
| `rsr.dbreakc1` / `rsr.dbreaka1` / `wsr.ibreaka0` | 4 / 1 / 1 | arms present |
| `rsqrt0.s` | 1 | arm present |
| **`salt` / `saltu`** | 6 | ⚠️ **no arm, and none was invented** |

`salt`/`saltu` have no `Inst` variant in `lp-xt-inst`, so they cannot be
assembled — and writing the encoding down by analogy is precisely M0's `rev8`
mistake. All six sites in this image are outside a sized code symbol, i.e.
literal-pool phantoms, so nothing executed is one; the test instead pins the
property that makes the gap safe to carry, that a word this hart cannot decode
is a named stop and never a silent wrong answer.

And three absences make this chip **simpler** than the classic: **no `rsil` at
all** (the classic has 82 — the S3 synchronises with `s32c1i`), no `f64*`
emulation block (the classic has 908 sites), and no `loop*`.

## ⚠️ A windowed call cannot cross a 1 GiB region

`retw` rebuilds the return address as `PC[31:30] ‖ a0[29:0]`, so caller and
callee must share the top two address bits. Code placed in the SRAM1 **D-bus**
view at `0x3FC9_0000` calling `memcpy` at `0x4005_6F44` returns to
`0x7FC9_0003` and dies in the ROM's debug vector on an undecodable word — a
wrong answer that looks like a machine bug. The firmware's own IRAM is at
`0x4037_xxxx` for exactly this reason, and so is every test in this crate that
calls the ROM.

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

⚠️ **And one is missing: `sensitive`.** P03's first strict stop is
`SENSITIVE + 0x04`, reached from the mask ROM (see the bring-up section
above), and the census could not have predicted it because it swept the
*application's* literals. P04 regenerates with `--pac esp32s3` to pick the
table up; the block itself is `0x600C_1000`
([`memmap::periph::SENSITIVE`](src/memmap.rs)).

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
