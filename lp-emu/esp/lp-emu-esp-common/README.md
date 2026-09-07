# `lp-emu-esp-common` — the Espressif SoC substrate

Everything an ESP machine emulator needs that is **not** a chip fact: the
bus, the MMIO decode table, the peripheral model, the accept-and-remember
register file, the bus trace, the host byte streams, and an ELF
program-header view.

MIT, inside the `lp-emu/` fence (`just lint-emu-fence`). See `../../README.md`
and `docs/adr/2026-09-06-lp-emu-home-and-mit-fence.md`.

## The neutrality rule

**This crate contains no chip numbers.** Not one base address, not one
register offset, not one interrupt source number, not one clock rate. A chip
crate — `lp-emu-esp32c6` first, whatever follows it beside — builds a
`SocBus` by registering its own RAM regions, MMIO windows and peripherals,
and everything here works the same for a C6, an S3 and a classic ESP32.

The same rule sends the generated register-name tables to the chip crate: a
register layout is chip-family data. `regnames` holds the type and the
lookup, never a table. (The two tables under `tests/regs/` are the
generator's proof, not the crate's data; see below.)

## The layering

```text
  machine (chip crate)   reset, memory map, interrupt matrix, ROM, CLI
        |
  SocBus                 RAM regions + MMIO decode + watchpoints
   |        |            + the unmapped policy + sideband
   |    Peripheral       read / write / on_event against a BusCx
   |        |
   |     RegFile         accept-and-remember, with a table of exceptions
   |
  Trace                  every MMIO access, with the PC, plus SPIN
  HostSinks              where a UART's bytes actually go
  Scheduler              (in lp-emu-core) guest time — plan PD5
```

### `bus` — regions and decode

RAM regions are sorted by base, looked up by binary search behind a
"last region hit" cache, and carry `exec` / `writable` flags: instruction
fetch comes only from an `exec` region (never from MMIO — a jump into
peripheral space is a wild branch, and answering it with a register's value
turns a crash into a puzzle), and a read-only region models ROM and the
flash cache window while `load_image` still places bytes into it from the
host side.

MMIO is a second sorted table. A peripheral's **index is its insertion
order and never moves**; the sorted view lives in a side table. That matters
because `event_id(peripheral, local)` packs the index into the scheduler's
opaque event tag, and an index that shifted when a lower-based peripheral
was registered later would silently re-point every already-scheduled event.

Byte, halfword and word accesses all reach the peripheral with their
`Width` and their lane: `esp-println` writes the USB-Serial-JTAG FIFO as a
32-bit word and the ROM writes UART0's FIFO as a byte, and both happen in
the same boot.

Three policies:

- **Unmapped is visible.** A read of an address nothing claims returns 0 and
  a write is dropped, but each distinct `(pc, address)` is logged once and
  every one is counted (`unmapped_reads`, `unmapped_writes`,
  `unmapped_sites`). Silence about a wrong memory map is what makes it cost
  a day.
- **Strict mode makes it fatal.** `set_strict(true)` turns the same access
  into `MemoryError::InvalidAccess` — the vision's honest-peripheral policy,
  available to a run that must not guess.
- **Watchpoints fire before the access.** esp-rtos's stack guard is a
  trigger on the guard word; a bus that performed the write and trapped
  afterwards would have already destroyed the evidence. NAPOT decodes as the
  RISC-V debug spec says (`mask = tdata2 ^ (tdata2 + 1)`).

MMIO writes set the **sideband** flag; RAM writes and MMIO reads do not. The
privileged stepper consumes it after store- and system-class instructions to
know whether the interrupt state may have moved.

### `periph` — what a peripheral may see

A `Peripheral` gets an offset, a `Width` and a `BusCx`: cycles, the issuing
PC, the hart index, the scheduler, the interrupt source lines, the trace and
the host streams. It never sees the hart's registers and never sees another
peripheral. It raises **source levels** (`irq.set_level(source, bool)`) and
schedules events; turning levels into a CPU interrupt number for a hart is
the chip's matrix, one layer up. `IrqLines` is chip-wide and per-source,
`BusCx.hart` says who is asking — which is the whole of plan PD6's
multi-hart shape until a second hart exists.

### `regfile` — accept-and-remember

Most of an SoC's register space is a place the firmware writes and later
reads back. `RegFile` is that, plus a short table of the exceptions:
read overrides ("this bit always reads as …"), write-one-pulse ("reads back
0 after a 1-write"), write-one-to-clear, read-only masks, reset values.
Every spin site the M3 discovery found is one line of it. Anything a block
does beyond that table gets a real type.

A read override leaves the stored value alone, so the trace still shows what
the firmware wrote and a real model can replace the stub later without
re-deriving state.

### `trace` — the bus log

The instrument the vendor emulator does not have. One line per MMIO access:

```text
cyc=41288 pc=0x42009a1c R4 TIMG0+0x068 rtccalicfg = 0x00000000
```

with a per-block filter, an `UNMAPPED` line class that ignores the filter,
and a **spin detector**: the same `(pc, address)` read N times (default
10,000) with no intervening write emits one line —

```text
cyc=999 pc=0x42009a1c SPIN SYSTIMER+0x004 unit0_op = 0x00000000 x1000
```

— which answers "which status bit is the blob waiting on?" in one line
instead of a day.

### `regnames` and the generator

`RegNames` is `offset → name`, binary-searched, with byte-lane offsets
rounded down to their register. Tables are **generated**, never written by
hand:

```bash
scripts/emu/pac-regnames.py           # regenerate
just lint-emu-regnames                # check (wired into `just check-lint`)
```

The generator reads the `#[doc = "0xNN - …"]` offset comments svd2rust
writes in the `esp32c6` PAC — the same source the M3 register inventory came
from — and every output file carries a provenance header naming the repo,
the crate path and version, the svd2rust version read out of the crate, and
the vendored licence (`licenses/esp-pacs-MIT.txt`), per
`docs/adr/2026-07-29-license-provenance-discipline.md`.

Names are block-local (`fifo`, `unit0load.hi`), not `uart0.fifo`: the block
token in a trace line comes from the peripheral *instance* (`UART0` vs
`UART1`), while a PAC register block is a *type* several instances share.
`RegNames::block` carries the type name for anyone who wants it.

The two tables under `tests/regs/` are the generator's proof — a flat block
and one with arrays and clusters — not this crate's data. From P4 on the
generator writes the real tables into the chip crate.

### `host` — where the bytes go

`HostSinks` is a set of named bidirectional byte streams; a peripheral holds
a `StreamId` and cannot tell whether its bytes end in a `Vec`, on stdout, or
(from M6) on a socket. The RX side is a **scripted** source by default
(`at_cycle → bytes`), because host connect timing was the one drift the
vendor emulator showed and plan PD5 says wall clock never enters the
machine.

### `elf` — the PT_LOAD view

`lp-riscv-elf` presents an emulator-guest memory image, not program headers.
`ElfImage` keeps `vaddr` and `paddr` separate (on ESP images they differ, and
using `vaddr` for both is how `.rtc_fast` ends up in the wrong place),
carries per-segment flags and the zero-fill tail, and looks symbols up by
name and by address — what the ROM intercept table and `--probe` need.

## Tests

```bash
cargo test -p lp-emu-core -p lp-emu-esp-common
```

`tests/elf_image.rs` reads a real rv32 firmware ELF **if one is already on
disk** and prints a notice and passes if not; building firmware from a host
crate's tests would make them depend on the rv32 toolchain. Point it
anywhere with `LP_EMU_TEST_ELF=<path>`.
