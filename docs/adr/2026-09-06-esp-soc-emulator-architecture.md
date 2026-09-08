# ADR: The ESP SoC emulator — machine over bus over hart, deterministic time, ROM loaded always

- **Status:** **Proposed (draft)** — to be accepted and backfilled at M7, once
  ROM-up boot exists. (`Proposed` is this repository's word for it, per
  `docs/adr/README.md`; the plan's word was "draft" and they mean the same
  thing.) Everything below is implemented and load-bearing today; what is
  deliberately missing is the half of the story only a boot-from-reset can
  tell, and the section that will hold it is marked.
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
  `docs/reports/2026-09-07-esp-emu-c6-spike.md`.

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

> **To be filled at M7.** That list is the cross-check: booting the same image
> from the reset vector through the real ROM and the IDF bootloader must
> arrive at the same machine state the direct loader hands over, and the boot
> log must diff against silicon's and esp-emu's. Until then the `boot-log`
> field class is graded `modeled` for the honest reason that there is no boot
> log at all — a direct load prints no banner.

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
  `SPIN SPI1+0x000 cmd` at 11 ms), RMT and a decoded WS281x frame (M5 — the
  `pin` class), an honest USB-Serial-JTAG with attach/detach over a control
  channel (M6 — the `usb-serial-jtag` class), ROM-up boot (M7 — the
  `boot-log` class).
- One open number, recorded rather than tuned: the idle heartbeat's
  `freeBytes` reads 266,688 here against esp-emu's 266,792 at the 5 s sample,
  closing to 4 B by 15 s. It does not move with the time grade and it is not
  the USB host state (both ruled out in M3 P6). Only a silicon capture of the
  memfs image can arbitrate it, which is a desk item.
- **To be revisited at M7**, when this ADR is finished: whether the direct
  loader's seven-item list is complete, and what the boot-log diff says.
