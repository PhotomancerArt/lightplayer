# ADR: The ESP SoC emulator — machine over bus over hart, deterministic time, ROM loaded always

- **Status:** **Proposed → Accepted (proposed at M7, 2026-09-08).** The draft
  said it would be finished once ROM-up boot existed. It exists: the chip
  boots itself from the reset vector through the real mask ROM and the real
  ESP-IDF second-stage bootloader into the application, and its boot log is
  silicon's line for line. The section that was marked *to be filled at M7* is
  filled below, and the two boot paths are now one architecture with two
  entries rather than one implemented path and one promise.
- **Date:** 2026-09-06
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None
- **Relates:** `2026-07-28-emu-core-crate-family.md` (the arch-neutral
  substrate and its neutrality rule); `2026-09-06-lp-emu-home-and-mit-fence.md`
  (the home and the licence fence); `2026-09-06-hardware-validation-system.md`
  (what a claim from this machine is worth);
  `2026-07-29-license-provenance-discipline.md` (why the vendored ROM and the
  PAC-derived tables carry provenance headers).
- **Context:** M3 of the 2026-09-06 esp-emulator plan (PD5, PD6, PD7), phases
  P1–P7, PRs #529–#555. The spike that preceded it is
  `docs/reports/2026-09-07-esp-emu-c6-spike.md`. **Amended by M6 P4**
  (PR #587): the honest-peripheral section gains per-register grades and
  `--strict-grade`, with the USB-Serial-JTAG block as the worked example.
  **Amended by M5 P4** (PR #598): the signal fabric, and what the `pin`
  grade means now that the shipped image's frame is held against the host
  oracle. **Completed by M7** (PR #596): ROM-up boot, the cross-check, and
  the two new seams the boot needed (`Bus::take_yield`,
  `RegFile::with_read_mirror`).

## Context

LightPlayer's firmware claims are chip claims: how much heap a project leaves,
what the WS281x line carries, whether the boot log says what it said last
month. Every one of them cost a desk sitting, a board, and a port nothing else
could hold — and the answers arrived days after the change that needed them.

The 2026-09-06 spike ran the shipped `fw-esp32c6` image under Espressif's
`esp-emu` 0.42.0 and established two things at once. That an emulator **can**
be trusted for some claims: the compile harness's heap figures were byte-equal
to silicon, all 184 per-tick values (§11.1). And that it must not be trusted
wholesale: the same run was 2.37x fast on compute and 14.5x on log-bearing
ticks, so the harness's own 5 ms slice budget passed there and failed on the
board; its USB-Serial-JTAG model asserts SOF forever, so the firmware served
into a void believing a host was attached (§4). A binary-only emulator can be
neither corrected nor graded, and `esp-emu` is binary-only.

So: build one, in the open, where every claim it makes is gradable and every
gap in it is visible.

## Decision

### The layering

Five layers; each knows strictly less than the one above it. The bottom two
know nothing about Espressif at all, and the third holds no chip numbers.

```text
lp-emu-validate      payload, configuration, transcript, replay, trust grading
  lp-emu-esp32c6     THE CHIP: memory map, mask ROM, reset state, peripherals,
                     the run loop, the CLI
    lp-emu-esp-common  bus + MMIO decode, Peripheral/BusCx, RegFile, trace with
                       its spin detector, host byte streams, ELF PT_LOAD view
      lp-riscv-emu     the hart: RV32IMAC executors, machine-mode CSRs, traps,
                       mret/wfi, hardware triggers, interrupt delivery
        lp-emu-core    guest memory, discrete-event scheduler, CycleModel
```

The line that matters most is between `lp-emu-esp-common` and
`lp-emu-esp32c6`: **no chip numbers cross it**, in either direction. A base
address, a reset value, a register name, a clock rate — all of them live in
the chip crate, and `memmap.rs` cites every one to
`esp-hal-1.1.1/ld/esp32c6/memory.x` or `esp-metadata-generated-0.4.0`. The
substrate offers a bus you register regions and peripherals with, and a
`Peripheral` trait whose implementations own their behaviour. A second chip is
a new crate at the `lp-emu-esp32c6` layer, not a fork of anything below it.

The hart is a *slot list* with one entry (PD6). The C6 has one core; the S3
and the classic have two, and the shape that admits them costs nothing now —
`harts[0]` everywhere, a scheduler on the bus rather than on the hart, and a
run loop that does not assume whose deadline it is computing.

### Time is a discrete-event schedule over a cycle model, and the wall clock never enters

Guest time is the scheduler's (PD5). The loop takes the nearest of the next
scheduled event, the stop cycle, the next probe and a slice cap; runs the hart
for that budget; fires what is due; and resamples. `wfi` jumps guest time
forward to the next event rather than sleeping. `micros = cycles / 160`, and
that is the only conversion.

The wall clock appears exactly once, as `--wall-timeout`, and it can **end** a
run, never change one. That is what makes two runs of the same image with the
same scripted input byte-identical, which is pinned by tests rather than
asserted (M3 P6: identical sha256, identical cycle and instruction counts,
across both gate images).

Time comes in **grades**, and no grade is a promise (vision "time is a graded
ladder, never a promise"):

- `t1` — one cycle per instruction. No cache, no flash wait states.
- `t2` — the measured per-class cycle model.

Both are configuration names in the validation system (`lp-emu:esp32c6:t1`,
`:t2`), so a transcript says which rung produced its numbers, and both are
graded `modeled` for timing because no transcript grades either yet. The first
point on that ladder, measured in M3 P7 against the same silicon capture:

| field | ours (`t1`) | silicon | esp-emu | ours/sil | emu/sil |
|---|---:|---:|---:|---:|---:|
| `build_us` | 550,958 | 568,757 | 54,361 | **0.97x** | 0.10x |
| `max_slice_us` | 10,676 | 11,724 | 4,625 | 0.91x | 0.39x |
| ticks 1–18 (compute), sum | 18,003 | 42,663 | 18,003 | 0.42x | 0.42x |
| ticks 19–92 (UART drain), sum | 532,895 | 526,048 | 36,312 | **1.01x** | 0.07x |
| ticks 19–92, median slice | 7,118 | 6,998 | 176 | 1.02x | 0.03x |

Two things to read off it. The compute ticks are byte-identical to esp-emu's,
because both grades count one instruction as one cycle — 0.42x is exactly the
error a missing cache and flash-wait model produces, and closing it is `t2`'s
rung and beyond. The log-bearing ticks land at 1.01x because UART0 drains at
the configured baud in *emulated* time: silicon spends 7 ms per console line,
esp-emu spends none, and this machine spends it. A model can be right about
time in one place and wrong in another, which is the whole argument for
grading per class rather than per emulator.

**No host gate runs on emulated microseconds** (PD9, vision D13). Memory
figures transfer; clocks do not.

### The ROM is loaded in every configuration, and the hook table starts empty

PD7, vision D6. The application calls into the mask ROM at runtime whatever
booted it: `rtc_get_reset_reason` from `__pre_init` *before `.bss` is zeroed*,
`ets_delay_us` from every clock path, `memcpy` and the `str*` family because
the linker resolves them there. So the ROM image is part of the memory map,
not an extra for a ROM-up boot. It is vendored under `lp-emu/esp/roms/` with
its Apache-2.0 licence and checksums, and a unit test re-derives the sha256
in-process so a swapped image fails a test rather than a boot at cycle
400,000.

A **hook** replaces a symbol's first instruction with `ebreak`, keeps the
original word, and lets a host function stand in. The table ships **empty**,
and the rule is: try the real ROM path first, and add a hook only when it
cannot be made to work by seeding a register a peripheral model owns.

Twice now the rule has held. `rtc_get_reset_reason` is three instructions
reading `LP_CLKRST.reset_cause & 0x1f`, so one reset value makes the real ROM
answer `POWERON`. The ROM console was pre-approved for hooking and did not
need it either: the routing global `ets_printf_uart` is 0 after a direct load,
which *is* UART0, so the real `uart_serial_tx_one_char` spins on
`status.txfifo_cnt` and stores into the real FIFO — which is also where the
7 ms-per-line floor above comes from. What P6 did add is the ROM's initialised
data: the non-allocated `PROGBITS` sections the ROM startup copies into HP
SRAM, without which `pp_rom_version` is NULL and the radio blob faults inside
`_vsnprintf`.

An empty hook table is the honest position. Every hook is a place the model
stops being the chip.

### Direct load now, ROM-up at M7

`loader.rs` reproduces what the ROM and the ESP-IDF second-stage bootloader
leave behind, because **the bootloader matters for memory**: its
`iram_loader_seg` reclaim is the app's second heap region, `.data` is not
copied by the app (`hal-defaults.x` hardcodes `__sdata = __edata = 0`), and
the core arrives at `_start` with `mstatus.MIE` already 1 — a hart left at the
architectural reset value would idle in `wfi` forever.

It also carries a written-down list of the seven things direct load does *not*
reproduce: the partition table, the MMU page table's *provenance*, the ROM's
console globals, the `rst:0x1 (POWERON)` banner, early RNG entropy, real
eFuse, and the derived reset cause.

**Amended at M4 (2026-09-07).** Two of the seven became things the loader
does, because the moment a firmware asks the flash *chip* a question the two
halves of the address space have to describe one board:

- `stage_image_in_flash` puts the image's flash-resident segments into the
  chip at `factory + (vaddr - 0x4200_0000)` and programs the cache MMU for
  them, so the `0x4200_0000` window really is served through the page table.
  The offsets are the loader's arithmetic, not an `esptool` image's layout,
  and no header, hash or partition table was consulted to choose them — which
  is precisely what M7's cross-check must catch.
- `seed_rom_flash_chip` writes the chip size into
  `rom_spiflash_legacy_data->chip_size`, in place of the bootloader's
  `esp_rom_spiflash_config_param`. The ROM's own default part is 2 MiB and
  `SPI_read_data` refuses any read past `chip_size`, so without it every
  `lpfs` read at `0x0031_0000` returns error 1 for a reason that has nothing
  to do with the filesystem.

### The cache is a fill, and the MMU's format comes from the ROM

The `0x4200_0000` window reads through `cache::CacheMmu`, whose every constant
is read off the vendored ROM ELF rather than a datasheet: `Cache_MMU_Init`
(`0x4002_7c76`) gives 256 entries and says zero is invalid, `Cache_MSPI_MMU_Set`
(`0x4002_7c90`) gives the entry format `page | encrypt<<10 | VALID<<9` and the
index arithmetic, `MMU_Get_Page_Mode` (`0x4002_75ea`) puts the page mode in
`mmu_power_ctrl[4:3]`. `translate` is the whole address path in one function,
because a later `t2` rung hangs its cache-miss wait states off that lookup.

The window is served as a **cache fill** — a valid page's flash bytes are
copied into the RAM region behind it when the table changes or the flash under
it is written — so instruction fetch stays a RAM read and the machine stays
fast enough to use. The cost is stated rather than hidden: the model is
**stricter than silicon about staleness**, since a real cache serves old bytes
until something invalidates it. No image in this plan writes a mapped page.

### Reset values are part of the model, and a wrong one is silent

M4 found this the expensive way. SPI1's `user` register resets to
`0x8000_0000` — `usr_command` already set — and the mask ROM's flash-read path
never sets it, because reset did. A register file that reset to zero therefore
issued every flash read with **no command phase** and moved no bytes; the
image booted, formatted `lpfs`, printed the right `[FS]` lines and the right
heap figures, and only the *second boot from the same chip* revealed it, by
reformatting a filesystem that was demonstrably on the disk.

The rule that follows: an accept block's reset values are as load-bearing as
its overrides, and they come from the PAC's `impl Resettable` — derived data
with the same provenance as the register names. Blocks written before M4 carry
only the reset values a boot was observed to need; that is a known gap, and
the next one to bite will be found the same way (see
`docs/defects/2026-09-07-accept-blocks-carry-only-the-reset-values-a-boot-needed.md`).

### ROM-up: the chip boots itself, and the two paths cross-check (M7)

`BootMode::RomUp` places **nothing**. A merged image — the second-stage
bootloader at `0x0`, the partition table at `0x8000`, the app in `factory`,
in one 4 MiB file `espflash save-image --merge` writes — goes into the flash
chip, the hart starts at `0x4000_0000`, and the mask ROM and the bootloader
do every one of the loader's jobs themselves, out of flash, for real.

Three properties of the architecture were load-bearing for that, and one seam
was missing.

**The ROM ELF is a debug view, not a dump, and the difference matters twice.**
`rom::seed_data` already existed because the ELF's `.data_*` and
`.data.interface.*` sections are non-allocated `PROGBITS` that no `PT_LOAD`
places. ROM-up needs the other half: `_init`'s copy loop reads those bytes
from **source** addresses (`0x4004_2196`..`0x4004_25a0`) that no `PT_LOAD` and
no section reaches at all. On silicon they are simply in the mask ROM; in the
ELF they exist only as the result. `rom::seed_data_image` derives the image
from the copy table and writes it back where the ROM will read it, so the
ROM's own loop copies exactly what the ELF says. Without it the loop wrote
zeros over everything, and the console died at `ets_ops_table_ptr`.

**A store can change what an address means.** Everything until M7 could be
answered by the hart or by the machine at a slice boundary. The cache MMU
cannot: the bootloader programs an entry and reads through the window a dozen
instructions later, in the same slice, and a machine that refills at the next
boundary serves stale bytes (`E boot_comm: mismatch chip ID, expected 13,
found 0`). `Bus::take_yield` is the seam — a peripheral calls
`BusCx::yield_to_machine`, the hart ends the slice after that store with
`SliceEnd::BusYield`, and the machine acts before the guest runs again. It is
checked only after an MMIO store, so a bus that never sets it costs nothing.

**One block cannot be honest by remembering.** The bootloader hashes the image
it is about to load, so `periph::sha` is the real SHA-1/224/256 compression
function driven exactly as the ROM's `ets_sha_process` drives it. It is the
only computed peripheral in the crate, and the reason is written at the top of
its file: an accept-and-remember SHA reads back zeros and the bootloader
refuses a good image.

**And a `done` bit sometimes has to go both ways.** `RegFile::with_read_mirror`
("these bits read as 1 exactly when those bits are set") exists because
`Cache_Freeze_ICache_Enable` spins until `l1_cache_freeze_done` is 1 and
`Cache_Freeze_ICache_Disable` spins until it is 0. No constant satisfies both;
a mirror says the true thing, which is that an operation with no duration in
this model has finished in whichever direction it was asked for.

#### What the cross-check found

`tests/rom_up_boot.rs` runs both paths on one image and compares.

- **The boot log is silicon's, line for line** — the ROM banner, `SPIWP`,
  `mode`/`clock div`, the three bootloader `load:` lines and `entry`, the
  bootloader's version and compile time, the SPI configuration, the whole
  partition table, `Loaded app from partition at offset 0x10000` and
  `Disabling RNG early entropy source`. Two lines are treated differently and
  each says why: `Saved PC:` is a memory of the previous run that a fresh chip
  cannot have, and the `esp_image: segment N` lines are compared against the
  image the machine was handed, because gating them on a transcript would gate
  on the linker (DD45).
- **The app's bytes are identical.** 2.4 MB of placed segments, byte for byte,
  at `[INIT] Board initialized`. The bootloader found the same bytes and put
  them in the same places the loader does.
- **DD40's two items hold.** The loader's synthetic flash offsets are the ones
  the real image has, because every mapped segment of an `esptool` image obeys
  the same 64 KiB congruence the loader's arithmetic assumes; and both paths
  leave the same chip size in `rom_spiflash_legacy_data->chip_size`.
- **The heap did not move.** The first heartbeat is byte-identical between the
  two paths (`freeBytes 265104`), and the residual 8 B against silicon is
  DD50's, unchanged — neither closed nor widened by having a real bootloader.
  The stack high-water is silicon's own figure exactly, 11908 B of 71512 B.

So the `boot-log` field class is graded `measured` for
`lp-emu:esp32c6:*` on this path: there is a boot log, it came from the real
ROM and the real bootloader, and a committed silicon capture is what it is
compared against.

#### What ROM-up does not reproduce

The list the direct loader carries has a counterpart, and it is short:

1. **`Saved PC:`** — `ASSIST_DEBUG.core_0_lastpc_before_exception` is zero on
   a machine that has never run. Silicon's boot after an espflash reset had
   the PC it interrupted in it.
2. **The download console does not answer commands.** The strap reaches the
   ROM's real download path and it prints `waiting for download`, but a
   scripted esptool SYNC gets no reply: the ROM begins with baud-rate
   auto-detection over `UART0`'s pulse-width counters (`rxd_cnt`,
   `low_pulse_cnt`, `high_pulse_cnt`), which the UART model does not drive.
   That is one measurable thing with a name, and plan two's shim is where it
   belongs.
3. **Three `pll_cal exceeds 2ms` lines**, from the ROM's `wait_rfpll_cal_end`
   polling an analog register the `I2C_ANA_MST` accept block cannot answer
   per-register (`docs/defects/2026-09-08-regi2c-is-one-data-register-not-a-register-file.md`).
(A fourth item lasted one day. The mask ROM's console drops a character
rather than waiting when the IN endpoint is not free, so the modelled
`IN_DRAIN_LATENCY_US` was losing runs of the densest output over USB while
UART0 kept everything. M5 P3's IN-FIFO auto-commit closed it from the other
side, and the two consoles now carry identical bytes across the whole boot
window; the gate reads the USB link, which is silicon's own, and asserts
UART0 agrees. The defect file keeps the bound silicon's capture puts on that
still-modelled number.)

#### Which path is the default, and for what (settled at M8)

Both are real boots of the same bytes and the cross-check above shows them
equivalent where it matters. The question M7 left open was which one anything
should *default* to, and M8 answered it by what each is for:

- **ROM-up is the walk's path** (`scripts/emu/m4-walk.sh`,
  `just walk-esp32c6-emu`). The walk exists to be the twin of a script whose
  first act is to flash and reset a board, and "the app was already in memory"
  is not nothing. It also means the boot chain is exercised on every walk,
  which is the only routine exercise it gets.
- **Direct load is the gates' path** — the `#[ignore]`d boot tests, the pin
  gate, and the heap ratchet (`just heap-budget-check-chips`). They run on
  every emulator PR, and G7-4 measured the idle heap byte-identical on the two
  paths, so the bootloader costs them seconds of wall clock and tells them
  nothing.

`LP_WALK_BOOT=direct` takes the fast path in the walk when iterating, and
`Payload::boot` (`BootPath::{Direct, RomUp}`) is how a recorded payload says
which one it means — `rom-up-boot` is the first and, for now, only `RomUp`
one.

### Honest peripherals: strict bus, `modeled` grades, and no invented answers

A peripheral is either **modelled** (a real type with behaviour and scheduled
events) or **accepted** (a register file that remembers writes, with a short
table of pinned bits, each citing the esp-hal line that reads or spins on it).
Everything else stays unmapped **on purpose**.

`--strict-bus` makes an access nothing claims fatal, with the address, the pc
and the symbol. That is the bring-up loop — run strict, read the first fault,
model that block, run again — and it is also the anti-lie: without it, an
unmodelled block answers zero and the guest believes it. Every gate transcript
is recorded strict.

Where a status bit must read something for the guest to make progress, the
answer is an **override with its evidence beside it**: the register, the
function that polls it, the disassembly of the poll, and what it reads. The
radio blob needed five, each found by running the image and reading a `SPIN`
line. A unit test refuses an override with no reason.

Consequently **every field class of `lp-emu:esp32c6:*` is graded `modeled`**,
each with its reason in `validate.toml`, even the one where the machine is
byte-equal to silicon on 372 values. Byte-equality on one payload is evidence,
written where a reader can weigh it; it is not a promotion. `measured` will
mean a transcript per class.

#### Amendment (M6, 2026-09-07): the grade is also per **register**

A block-level grade turned out to be too coarse to be useful, in both
directions. "Modelled" covered a USB-Serial-JTAG block whose data path four
transcripts had exercised byte for byte *and* twenty registers in the same
block that no driver on this chip has ever touched. Saying one word about
both hides the difference that matters when somebody asks "can I trust what
this run did?".

So a peripheral may publish a **`RegGrades`** table — register offset to
`RegGrade::{Modeled, Documented, Measured}` — in its own file header, where a
reviewer reads it beside the code it grades, and `--strict-grade <level>`
refuses an access to a register below that level, before the peripheral sees
it, reported like a strict-bus stop with the register's name and its grade.

Three rules make it a policy rather than a feature.

1. **A grade moves only with a transcript.** `documented` means a document
   states the behaviour and the implementation follows it — the SOF period is
   the USB full-speed frame, cited to the firmware's own module doc.
   `measured` means a committed transcript exercised it. Nothing is promoted
   because it looks right or because a test passes.
2. **A grade is as coarse as the table, and the file says where it is coarser
   than the evidence.** `RegGrades` is per register; three of USB_DEVICE's
   `measured` registers are measured only in bits 1–3, and the header names
   the bits it means and the bits it does not. A per-bit table is the obvious
   refinement the day something depends on it.
3. **The level applies to the blocks that published a table.** A block with
   no table is passed over, because "nobody graded this block" is a different
   statement from "this block is modelled" — and conflating them made
   `documented` stop at the first MMIO access of any boot, on an accept table
   nobody had said anything about, which measured how much of the chip had
   been graded rather than what the run was allowed to trust. The run report
   names the blocks it checked, so an ungraded one reads as an unanswered
   question and never as a pass.

**The worked example is USB_DEVICE** (`periph/usb_sj.rs`, M6). `ep1`,
`ep1_conf` and the four `int_*` registers are `measured` — SOF present while
attached and gone when the cable is out, `serial_in_ep_data_free` returning
only once a host has drained the packet, `serial_in_empty` completing
esp-hal's write future, `serial_out_recv_pkt` on the host's own bytes, all
under `lp-emu/transcripts/esp32c6/`. `fram_num` and `conf0` are `documented`.
Twenty are `modeled`, with one reason for all of them: neither esp-hal 1.1.1
nor esp-println 0.17 touches them on the C6. The shipped image runs 5.5 s
attached under `--strict-grade documented` and crosses none of them.

**The two levels do not promote each other.** A class in `validate.toml` is
graded for a whole configuration and a register is graded in its block; six
`measured` registers do not make the `usb-serial-jtag` *class* `measured`,
and it is not. What would is a silicon transcript of that class — which is
exactly what the amendment is for: it lets the block say what it has earned
without letting the configuration overclaim.

#### Amendment (M5, 2026-09-07): the signal fabric, and the pin grade

A peripheral never sees another peripheral, and the pin is the place that
rule bites: the RMT block cannot read `GPIO.func_out_sel_cfg[18]` to learn
which pad carries its waveform, and the GPIO block cannot ask the RMT what
level its signal is at. The routing is one fact two blocks share, which is
the shape the interrupt matrix already had (DD22), and it gets the same
answer — **one state on the bus, register views writing into it**. The
**signal fabric** (`lp-emu-esp-common::pins`, `BusCx.pins`; DD34 e) holds
pads, signals and edges and no chip numbers: the chip's GPIO block is a
routing *view* that writes `route`/`set_gpio_out` into it, an output
peripheral `drive`s its signal into it and never learns whether anyone is
listening, and the machine drains the edges every slice into whatever
watches the wire — a WS281x decoder per routed pad (`strip::ws281x`,
±150 ns, the datasheet's tolerance and not a knob) and a raw pin log. The
decoder knows nothing about who produced the edges, which is what makes its
frames a second reading rather than the peripheral's own word log restated.

What the `pin` class is graded on, then, and why it is still `modeled`. Two
captures are committed beside their transcripts (the additive `pins`
companion, E3). On `rmt-chase` the guest's own per-frame checksum agrees
with the pad on all 768 frames. On `shader-oracle-walk` the **shipped**
image's first lit frame off gpio18 is byte-equal to the host oracle's
`[ORACLE] rgb=` and `[ORACLE-RV32] rgb=` — the walk's own PASS criterion
for the S3, with the decoder standing in for the frame-dump line the C6
does not have — and every later frame is the same bytes, on both time
grades. The oracle is rendered by two host engines that never touched the
machine, so this is the nearest independent check there is short of an
instrument. It is not a measurement, because both readings of the *pad*
are ours: the RMT model produces the waveform and our decoder reads it.
By the rule above, byte-equality is evidence weighed in the `because`, and
`measured` waits for a silicon pin transcript — a logic analyser on the
desk, or the C6 `frame-dump` port (M8's) read beside the decoder.

### Provenance on everything derived

Per `2026-07-29-license-provenance-discipline.md`. The register-name tables in
`src/regs/` are generated from the esp32c6 PAC's svd2rust offset comments by
`scripts/emu/pac-regnames.py`, carry a provenance header naming repo, path and
version, and are checked by `just lint-emu-regnames` — a hand edit is reverted
by the next regeneration and takes its provenance with it. The ROM ELFs are
committed verbatim from `espressif/esp-rom-elfs` release `20260528` with
LICENSE and `SHA256SUMS`, never edited, re-derivable by
`scripts/emu/fetch-rom-elfs.sh`. The whole family is MIT behind
`just lint-emu-fence` and may not import a product crate — which is why the
validation system's payload registry *mirrors* `fw-checks` instead of
importing it.

## Alternatives considered

**Keep using `esp-emu`.** It is free, it exists, and it was right about memory.
It is also binary-only: a wrong answer cannot be fixed, a missing peripheral
cannot be added, and — the decisive one — nothing in it can be *graded*, so
every number needs a desk sitting to believe anyway. It stays as a second
oracle, `measured` for memory on the one payload a transcript proves it on,
and its transcripts are committed beside ours.

**QEMU.** Rejected earlier on licence grounds (see the licence ADR); also, its
Xtensa support lacks RMT and USB, which is most of what this plan needs, and
its C6 support would still leave the peripheral work to us.

**A cycle-accurate model.** Not attempted, and not planned. The graded ladder
exists because "how many microseconds" is the question this machine is worst
at and the product needs least: `t1` and `t2` bound it honestly, and PD9 keeps
a host gate from depending on it.

**One flat crate.** Rejected for the reason the substrate/chip line exists:
the second chip is the test of the design, and a flat crate fails it.

## Consequences

- The shipped firmware image boots to its idle loop on a host, in ~15 s of
  wall time, with no board. `just emu-c6 <elf>` for one run,
  `just test-emu-c6` for the gates.
- The M3 gates are committed transcripts, replayed by `cargo test` with no
  firmware build: harness memory byte-equal to silicon (372/372) and to
  esp-emu, the shipped image's boot heap and stack figures against spike
  report §5.4, and negative controls that fail on one wrong digit.
- Everything the machine claims is `modeled`, on purpose, and the path to
  `measured` runs through transcripts rather than through confidence.
- The gaps are visible and named, each owned by a milestone: flash and the MMU
  windows (M4 — the flash-backed shipped image still stops at
  `SPIN SPI1+0x000 cmd` at 11 ms), RMT and a decoded WS281x frame (M5, done — the
  `pin` class, `modeled` with the oracle equality as its evidence), an honest USB-Serial-JTAG with attach/detach over a control
  channel (M6 — the `usb-serial-jtag` class), ROM-up boot (M7 — the
  `boot-log` class).
- One open number, recorded rather than tuned: the idle heartbeat's
  `freeBytes` reads 266,688 here against esp-emu's 266,792 at the 5 s sample,
  closing to 4 B by 15 s. It does not move with the time grade and it is not
  the USB host state (both ruled out in M3 P6). Only a silicon capture of the
  memfs image can arbitrate it, which is a desk item.
- **Revisited at M7 and answered: both.** The direct path stays, because it is
  what M4's and M6's gates run and because it is two orders of magnitude
  cheaper to start; ROM-up is what proves the direct path is telling the truth,
  and the cross-check is the proof. The next question — whether a
  ROM-up boot should become the *default* for a walk — belongs to M8, where
  the walk is.
  (This replaces the draft's "to be revisited at M7: whether the direct
  loader's seven-item list is complete, and what the boot-log diff says." It
  was not complete — the ROM's own data image was missing from it, because
  nothing on the direct path executes `_init` — and the boot-log diff is
  above.)
