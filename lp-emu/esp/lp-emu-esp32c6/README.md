# `lp-emu-esp32c6` — the ESP32-C6 machine

This is where the chip numbers live. `lp-emu-esp-common` supplies the bus,
the peripheral model, the trace and the ELF view and knows nothing about any
chip; `lp-riscv-emu` supplies the privileged hart and knows nothing about
MMIO. Here they are put together with a memory map, a mask ROM, a reset state
and a run loop, and the result takes a `fw-esp32c6` binary.

```bash
cargo run -p lp-emu-esp32c6 --release -- \
    --elf target/riscv32imac-unknown-none-elf/release-esp32/fw-esp32c6 \
    --strict-bus --trace --timeout 100ms
```

## The machine

`Esp32C6Machine` is one hart, one bus and one schedule. `harts` is a slot
list with a single entry (plan PD6): the C6 has one core, and the shape is
what lets a second chip's machine reuse this loop without reshaping it. The
schedule lives on the bus, because that is where the peripherals that write
to it are.

The loop (plan PD5):

```text
loop {
  deadline = min(next scheduler event, stop cycle, next probe, slice cap)
  match hart.run_slice(bus, deadline - now) {
    BudgetExhausted => fire every due event, then resample the matrix and poll
    Wfi             => jump guest time to the next event, then the same
    Ebreak { pc }   => the ROM hook table gets first refusal, else deliver it
    Fault(f)        => stop
  }
}
```

**Wall clock never enters the machine.** Guest time is the scheduler and the
hart's cycle model, and nothing else, so two runs of the same image with the
same scripted host input are byte-identical — pinned by a test on the shipped
image, not asserted. `--wall-timeout` is the single exception and it is a
safety net: it can end a run, never change one.

Time comes in two grades, both of which are just the hart's cycle model:
`t1` (`lp-emu:esp32c6:t1`) counts instructions, `t2` uses the measured
per-class model. The CPU is 160 MHz, so `micros = cycles / 160`. Neither
grade is a claim about milliseconds on silicon — see the vision's "time is a
graded ladder, never a promise".

## The memory map

`memmap.rs` holds every base and length, each cited to
`esp-hal-1.1.1/ld/esp32c6/memory.x` or `esp-metadata-generated-0.4.0`.
Nothing else in the crate writes an address literal.

```text
0x4000_0000 +0x4_AC00   ROM_MASK      mask ROM code     exec, read-only
0x4004_AC00 +0x0_5400   DROM_MASK     mask ROM data     read-only
0x4080_0000 +0x8_0000   HP SRAM       512 KiB           exec, writable
0x4200_0000 +0x80_0000  flash cache   IROM + RODATA     exec, read-only
0x4280_0000 +0x80_0000  DROM window   kept readable     read-only
0x5000_0000 +0x0_4000   LP SRAM       RTC_FAST, 16 KiB  exec, writable
0x2000_0000, 0x6000_0000                                MMIO windows
```

**HP SRAM is one region.** The linker script cuts it into the app's RAM
(ending at `0x4086_E610`), the bootloader's reclaimed `dram2_seg` (the second
heap region) and the ROM's own data and stack above that — but those are
documentation, not decode. The hardware has one 512 KiB window, and modelling
three would only make a one-byte overrun fault where silicon reads a byte.

`MEM_INTERNAL2` (`0x600F_E000`) is deliberately unmapped. It sits inside a
declared MMIO window, so an access to it reads as "an unmodelled block"
rather than as a wild pointer — which is the diagnosis you want.

## The ROM is loaded in every configuration

Plan PD7, vision D6. The application calls into the mask ROM at runtime
whatever booted it: `rtc_get_reset_reason` from `__pre_init` *before `.bss` is
zeroed*, `ets_delay_us` from every clock path, `memcpy` and the `str*` family
because the linker resolves them there. So the ROM image is part of the memory
map, not an extra for a ROM-up boot. What is optional is running the boot
chain from reset, which is M7.

The image is vendored at `../roms/` with its licence and checksums; a unit
test re-derives the sha256 in-process, so a corrupted or swapped ROM fails the
build's tests rather than a boot at cycle 400,000.

Three things about the real image that a loader written from the memory map
alone would get wrong, and which `rom.rs` handles by walking region
boundaries:

1. **Seventeen of its 21 `PT_LOAD`s are empty** (`filesz = memsz = 0`) at
   `vaddr = 0`. Skipped and counted, so "21 program headers, 4 placed" is not
   a discrepancy anyone has to chase.
2. **One segment straddles two regions.** `0x4004_8400 + 0x5fa8` starts inside
   ROM_MASK and ends inside DROM_MASK, because the `0x4004_AC00` boundary is
   esp-hal's line, not the ROM linker's.
3. **Its `.bss` reaches into HP SRAM** (`0x4086_ad08 + 0x14da4`), across what
   the app calls RAM. That is correct, and it is why the ROM is loaded
   **before** the app: the direct loader then overwrites what the app owns,
   exactly as the real bootloader does.

### The trampoline table

The addresses in `esp-rom-sys`'s linker script are **not** the
implementations. `0x4000_0018` is `__call_rtc_get_reset_reason`, a `jal` to
`rtc_get_reset_reason` at `0x4001_9680`. Both symbols are real and neither is
the other. A test decodes every slot's `jal` and asserts it lands on the
same-named body, which is how a ROM swapped for another revision is caught by
name rather than by a wrong jump.

### Hooks, and why the table is empty

A hook replaces the first instruction at a symbol with `ebreak` and keeps the
original word. The hart returns `SliceEnd::Ebreak` with the `pc` unadvanced
and uncharged; the machine gets first refusal, asks the table, and if a hook
claims the `pc` it runs the host function and performs the `ret` itself. Every
other instruction costs nothing.

The table ships **empty**, and the rule is: try the real ROM path first, and
add a hook only when it cannot be made to work by seeding a register a
peripheral model owns.

The first candidate is already answered that way. `rtc_get_reset_reason` is
three instructions —

```text
lui  a5, 0x600b0
lw   a0, 0x410(a5)
andi a0, a0, 31
ret
```

— so it returns `LP_CLKRST.reset_cause & 0x1f`, and one reset value on that
block (`with_reset(0x010, 1)`) makes the real ROM answer `POWERON`. No hook.
`tests/rom_reset_reason.rs` pins both halves, including the failing one:
against an unseeded block the ROM says "no reset" on a genuine power-on,
`.rtc_fast.persistent` is never zeroed, and the firmware's `resetReason` is
quietly wrong.

Note the base — `0x600B_0410` is **LP_CLKRST** (`0x600B_0400`), not LP_AON,
which is at `0x600B_1000`. The generated register-name table is what caught
that.

## Direct load

`loader.rs` reproduces what the ROM and the ESP-IDF second-stage bootloader
leave behind, because **the bootloader matters for memory**: its
`iram_loader_seg` reclaim is the app's second heap region, `.data` is *not*
copied by the app (`hal-defaults.x` hardcodes `__sdata = __edata = 0`, so the
bytes have to already be there), and the core arrives at `_start` with
`mstatus.MIE` already 1 — nothing in the esp-hal stack ever sets it, so a hart
left at the architectural reset value would idle in `wfi` forever.

The file also carries a written-down list of the seven things direct load does
**not** reproduce — the partition table, the MMU page table, the ROM's console
globals, the `rst:0x1 (POWERON)` banner, early RNG entropy, real eFuse, and
the derived reset cause. That list is the seed for M7's cross-check, and it is
worth more than the code around it.

## Peripherals — there are none

Not one. The phase gate is the first access nothing claims, and a machine that
had guessed at a few register blocks would have moved that fault somewhere
less informative. The interrupt matrix (`intmatrix.rs`) is likewise a seam
with no semantics behind it. P5 fills both in.

What P4 does own is the *order* they will be registered in.
`event_id` packs a peripheral's index into the scheduler's event tags, and the
index is insertion order — so re-sorting the registrations (by base address,
say, for tidiness) would silently re-point every already-scheduled event.
`PERIPHERAL_REGISTRATION_ORDER` is that order written down, and the builder
refuses a sequence that is not a subsequence of it. Adding a block is an edit
to that list, which a reviewer sees, rather than a call site nobody diffs.

## Snapshot

In memory only — there is no file format in M3, on purpose. A snapshot whose
format is committed has to survive every change to every peripheral's state
blob; one that only travels inside a process is free.

It is not checked by inspection. The test runs to cycle N, snapshots, runs on
to M, restores, and runs to M **again**, requiring an identical trace:
anything the snapshot forgot shows up as a diverging line, because the trace
is a function of every access the machine makes.

## The CLI

```text
lp-emu-esp32c6 --elf <app.elf> [--rom <path>] [--time-grade t1|t2]
    [--timeout 5s|1500ms|900us] [--wall-timeout <s>] [--exit-on <substr>]
    [--uart0 stdout|file:<path>] [--uart0-script <file>]
    [--efuse-mac a0:f2:62:87:b4:8c] [--efuse-rev 0.2] [--seed <u64>]
    [--trace [BLOCK,BLOCK…]] [--trace-file <path>] [--strict-bus]
    [--probe <symbol>@<ms>] [--hooks] [--map]
```

Every timeout is **emulated** time, so a run is the same run on a laptop and
on a loaded CI box. A duration without a unit is refused rather than guessed:
`100` could be anything, and guessing would run a thousand times too long or
too short.

Exit codes are a contract: `0` on an `--exit-on` match or a clean timeout,
`2` on a hart fault (symbolized against the app ELF), `3` on a strict-bus
violation, `4` on the wall-clock safety net.

## Bring-up loop

Unmapped is visible, and strict makes it fatal. The loop is: run
`--strict-bus --trace`, read the first fault, model that block, run again.

```text
STRICT-BUS Read inside a declared MMIO window — an unmodelled block
  of 4 bytes at 0x600b0410 from pc=0x40019684 (rtc_get_reset_reason+0x4)
```

Without `--strict-bus` the run carries on with unmapped reads answering zero,
which is how far the machine gets before a single block is modelled — far
enough, on the shipped image, to walk the whole documented boot sequence.

## Tests

`cargo test -p lp-emu-esp32c6` runs everything that needs no firmware. The
boot tests are `#[ignore]`d, because a workspace test run must not start a
cross-target firmware build; `just test-emu-c6` sets the environment and runs
them. `test_support` resolves the ELF from `LP_EMU_C6_ELF_<SLUG>`, then the
conventional target path, and only builds when `LP_EMU_BUILD_FW=1`.

## Provenance

- The mask ROM is Apache-2.0, from `espressif/esp-rom-elfs` release
  `20260528`. See `../roms/README.md`.
- The register-name tables in `src/regs/` are generated from the `esp32c6`
  PAC's svd2rust offset comments by `scripts/emu/pac-regnames.py` and carry
  the provenance header `docs/adr/2026-07-29-license-provenance-discipline.md`
  requires. Never hand-edit one; `just lint-emu-regnames` catches it.
- The crate is MIT, as a unit with the rest of `lp-emu/`. See
  `../../README.md` and `just lint-emu-fence`.
