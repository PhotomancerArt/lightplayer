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

## Peripherals

`periph/` is the boot set: what the no-radio image
(`--no-default-features --features esp32c6,server,memory_fs`) touches between
`_start` and the esp-rtos idle loop, with every register access either
**modelled** (a real type with behaviour and scheduled events) or **accepted**
(a `RegFile` that remembers writes, with a short table of pinned bits, each
citing the esp-hal line that reads or spins on it). Everything else stays
unmapped on purpose, so a strict run stops on the first block a later
milestone owns.

| Block | Base | Grade | What is real |
|---|---|---|---|
| `INTERRUPT_CORE0` | `0x6001_0000` | modelled | `core_0_intr_map[0..77]` (31 = disabled), live `core_0_intr_status[0..3]` — a view into the matrix |
| `PLIC_MX` | `0x2000_1000` | modelled | `enable`, `type`, `clear` (pulse, reads 0), `pri[0..32]` (4 bits), `thresh` (8 bits, resets **1**), `emip_status`; fires iff `enable && pri >= thresh`; ties → higher number (*modeled*); edge type stored, modelled as level |
| `INTPRI` | `0x600C_5000` | modelled | `cpu_intr_from_cpu[0..4]` bit 0 = level of source 22+n; rest accept |
| `SYSTIMER` | `0x6000_A000` | modelled | two 52-bit units at cycles/10 (16 MHz), `unit_op.update`/`value_valid`, three comparators armed by `comp_load`, period mode, `int_*`; sources 57..59 |
| `TIMG0` | `0x6000_8000` | modelled | **the esp-rtos tick**: T0 at XTAL/2 (*modeled*: esp-hal's default source and prescaler reading), `update` pulse, alarm as a scheduled event, `alarm_en` self-clearing, auto-reload; RTC calibration as a timed event (value *modeled* `40e6·max/136e3`); MWDT accepted behind its key, arming leaves a `WDT ARMED` note |
| `TIMG1` | `0x6000_9000` | modelled | same type, sources 54/56; the firmware only disables its WDT |
| `LP_WDT` | `0x600B_1C00` | modelled | RWDT: key-gated config/feed, stage-0 expiry as a scheduled event at `hold·2/136 kHz` (*modeled* from esp-hal's `>> 1` shift); a reset action ends the run with `Outcome::Reset`; interrupt action drives source 18; SWD accepted; `+0x54` named `reserved_054` |
| `EFUSE` | `0x600B_0800` | modelled | memory seeded from `--efuse-mac`/`--efuse-rev` in esp-hal's byte order (`rd_mac_spi_sys_0/1/3`) |
| `RNG` | `0x600B_2800` | modelled | `rng_data` (+0x08) = xorshift64\* from `--seed`; deterministic by design; other `LP_PERI` offsets accept |
| `LP_CLKRST` | `0x600B_0400` | accept | `reset_cause` seeded 1 (POWERON) — the mask ROM's `rtc_get_reset_reason` reads it; `lp_clk_conf` = RC_SLOW |
| `PCR` | `0x6009_6000` | accept | `sysclk_conf.clk_xtal_freq` pinned 40; `cpu_waiti_conf.cpu_wait_mode_force_on` pinned 0; `timergroup.timer_clk_conf` reset = XTAL |
| `I2C_ANA_MST` | `0x600A_F800` | accept | `ana_conf0.cal_done` pinned 1; `i2c_ctrl(0/1).busy` pinned 0; `ana_conf2` reset 0 (master 1) |
| `LP_I2C_ANA_MST` | `0x600B_2400` | accept | `i2c0_ctrl.I2C0_BUSY` (bit 25) pinned 0 — the bench's fourth spin site |
| `ASSIST_DEBUG` | `0x600C_2000` | accept | `cpu0.debug_mode` pinned 0 (no debugger: watchpoints arm, `wfi` runs) |
| `GPIO` | `0x6009_1000` | accept | `in_` and `pcpu_int` pinned 0; `enable`/`out` writes are trace lines (pins: M5) |
| `IO_MUX` | `0x6009_0000` | accept | all 31 pads at reset `0x0800` |
| `PMU`, `LP_AON`, `LP_APM`, `LP_APM0`, `HP_APM`, `MODEM_SYSCON`, `MODEM_LPCON`, `APB_SARADC`, `HP_SYS`, `TEE`, `LP_TEE`, `LP_IO`, `LP_TIMER`, `EXTMEM` | — | accept | written by `esp_hal::init`, read back as written; `LP_AON.store1` carries the calibration value |
| `UART0`, `UART1` | `0x6000_0000/1000` | accept | the FIFO, thresholds and `RXFIFO_TOUT` are P6 |
| `USB_DEVICE` | `0x6000_F000` | accept | reads 0: "host absent, FIFO full" — esp-println spins 50,000 iterations once and then drops output; the honest model is P6/M6 |
| `SPI0`, `SPI1` | `0x6000_2000/3000` | accept | a flash access spins on `SPI1.cmd` (the `SPIN` line names it) until M4 |
| `RMT` | `0x6000_6000` | accept | `Rmt::new` runs in every image; channels, blocks and the WS281x waveform are M5 |
| radio window | `0x600A_0000..9800` | **unmapped** | `IEEE802154` / WiFi MAC/BB — P6's stub; the shipped image's strict run stops here |

Two values worth repeating because they are *modeled*, not measured: the RTC
slow clock is taken as 136 kHz (so the calibration value is 301,176 for 1,024
cycles, and the RWDT's 30 s boot timeout is 30 s), and a comparator or alarm
whose target is already past fires immediately rather than never.

The tick is TIMG0 T0, not a SYSTIMER comparator: `esp_rtos::start` is handed
`timg0.timer0` (`board/esp32c6/init.rs`). esp-rtos 0.3.0's tick is one-shot —
`arm_next_wakeup` programs the next wakeup and `timer_tick_handler` re-arms —
so there is no periodic 10 ms tick: the gaps between ticks are the sleeps the
firmware asked for, up to 250 ms while every task sleeps.

The registration *order* is a contract. `event_id` packs a peripheral's index
into the scheduler's event tags, and the index is insertion order — so
re-sorting the registrations would silently re-point every already-scheduled
event. `PERIPHERAL_REGISTRATION_ORDER` is that order written down, the builder
refuses a sequence that is not a subsequence of it, and `periph::boot_set`
registers exactly it. `Esp32C6Builder::bare()` builds the map and the ROM with
no peripherals at all, for tests that bring their own.

### Two seams the peripherals needed

The interrupt matrix on the bus is the single copy of the routing
configuration (plan DD22); `INTERRUPT_CORE0` and `PLIC_MX` are register
*views* that write into it through `BusCx::matrix` and read back from it.
One state, nothing to keep in sync, and `Bus::pending_cpu_interrupt` stays a
pure function of the source levels.

A peripheral cannot reset the chip. When the RWDT's stage 0 expires with a
reset action it leaves a `MachineRequest::Reset` on the bus, and the run ends
with `Outcome::Reset` (exit code 2): the emulator reports the reboot it cannot
yet perform, which is also the more useful answer during bring-up.

### Reading the trace

Beyond the MMIO lines, three kinds of note appear in the same stream:

- `SPIN <BLOCK>+<off> <name> = <value> x<n>` — the same register read `n`
  times in a row from the same PC with no write in between. Unfiltered.
- `WATCHPOINT slot=<n> armed at <tdata2> napot store` / `disarmed` — a
  trigger CSR write that changed a slot's effective watchpoint (esp-hal
  rewrites all four on every context switch; only changes are logged).
- `<BLOCK> WDT ARMED` / `LP_WDT RWDT EXPIRED: …` — the watchdogs.

A 3 s no-radio run with `--trace SYSTIMER,PLIC_MX,INTPRI,INTERRUPT_CORE0,LP_WDT,TIMG0`
is about 780 k lines, half of them the RTC-calibration poll at boot. Without
a filter it is 18 M lines; write it to a file on a disk with room.

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
