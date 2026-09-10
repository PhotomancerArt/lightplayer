# `lp-emu-esp32v3` — the classic ESP32 (v3) machine

This is where the classic's chip numbers live. `lp-emu-esp-common` supplies
the bus, the peripheral model, the trace and the ELF view and knows nothing
about any chip; `lp-xt-emu` supplies the Xtensa hart and knows nothing about
MMIO. Here they are put together with a memory map, a mask ROM, a reset state
and a run loop, and the result takes a `fw-esp32v3` binary.

It is `lp-emu-esp32c6`'s twin, deliberately: same shape, same module names,
different silicon.

> **M3 P2.** What exists today is the memory map, the generated register-name
> tables, the vendored mask ROM, the bus, the two-slot machine, the run loop,
> the snapshot and a CLI that runs to the first strict stop. **No peripheral
> is modelled.** The sections below that name a later phase are stubs, and
> they say so rather than describing a machine that does not exist yet.

## The machine

One bus, **two** `XtHart<SocBus>` slots, one schedule.

The layering is `lp-emu-esp32c6/README.md:15-69`'s, and the classic differs in
exactly two ways:

- the hart is `lp_xt_emu::mach::XtHart<SocBus>` rather than
  `lp_riscv_emu::mach::MachineHart<SocBus>`, and
- **`harts` has two slots, and slot 1 is stalled for the whole of M3.**

### What "stalled" means here

The classic is dual-core. Slot 1 is *constructed* — it holds architectural
state, it appears in a snapshot, `--probe` and the run report name it — and it
is **never given a slice**, so it consumes no guest time. `Machine::run_until`
asserts `stalled[1]` before every slice rather than assuming it.

That is Q5's answer for this milestone, and it is a supported configuration of
the firmware rather than a hole: `start_app_core` times out and the image takes
its documented single-core fallback (`lp-fw/fw-esp32v3/src/main.rs:838-845`,
which prints `[INIT] APP core unavailable; RMT ISR on PRO core (single-core
semantics)`). **M4 is where core 1 runs**, with an interrupt matrix and the
deterministic quantum interleave.

On silicon a stalled core is held by two register pairs, and P4 wires
`Machine::core_stalled` to them: `RTC_CNTL.options0.sw_stall_appcpu_c0` plus
`RTC_CNTL.sw_cpu_stall.sw_stall_appcpu_c1` (both halves of the key) and
`DPORT.appcpu_ctrl_c.appcpu_runstall`. In P2 none of those registers exists,
so it is a plain machine field.

### The reset state, and the boot state

`XtHart::new` leaves the **architectural** reset value: `PS = 0x1F`,
`VECBASE = 0x4000_0000`, `pc = 0x4000_0400`. A rom-up run starts exactly there
and seeds nothing — the ROM's own reset vector sets `PS` itself.

A **direct load** seeds `PS = PS_BOOT = 0x0006_0020` (`WOE | UM |
CALLINC(2)`), the Xtensa twin of the C6's `mstatus = 0x1888`. The `CALLINC(2)`
is not decoration: the IDF bootloader reaches the application's entry through
an ordinary C call, which on the windowed ABI is `callx8`, so the app's
`Reset:` runs as frame **2** and the bootloader's frame 0 stays live behind it.

Which is why a direct load also seeds a **boot frame** (`BootFrame`):

- `a1`, the outermost live frame's stack pointer, and
- the four words of that frame's base save area at `[a1-16, a1)`.

Without them `a1 = 0`, and nothing notices until the first register spill —
`_WindowOverflow8`'s `l32e a0, a1, -12` then reads `0xFFFF_FFF4`, faults, and
the fault's own spill faults again. A double exception, forever, nowhere near
its cause.

P2 pins `a1` to the mask ROM's own PRO-core stack top, `__stack`, **resolved
from the vendored ROM ELF** (`0x3FFE_3F20` — the same address
`third_party/esp-hal/ld/esp32/memory.x:32` derives `reserved_rom_stack_pro`
from). That is the right shape and a cited value; it is **not yet the
bootloader's own SP**, which P3 pins from the bootloader disassembly or from a
rom-up run that gets that far. `BootFrame` is a builder parameter for exactly
that reason.

### Time

`TimeGrade` is **t1 only** in M3: cycles are instructions, and
`micros = cycles / 240` (`memmap::CPU_HZ`). `--time-grade t2` is refused with a
message naming the calibration that does not exist: there is no measured
Xtensa per-instruction-class table in this repo, and six months from now an
invented one would be indistinguishable from a measured one. The classic does
have a better calibration source than the C6 ever had — `CCOUNT` is CPU cycles
at 240 MHz — but that is an M5/M7 opportunity, not an M3 one.

### Two decisions the bus makes, and one gap it has

**RTC fast memory's instruction-bus view is not mapped.** `memory.x:51` and
`:54` put the same 8 KiB block behind `0x400C_0000` (I) and `0x3FF8_0000` (D).
`SocBus` cannot express that: its regions are asserted non-overlapping and its
bytes live in one flat arena keyed on `address - arena_base`, so two regions
are two independent stores and a write through one is invisible through the
other. So the D-bus view is mapped and the I-bus view is **named and
unmapped** — the SRAM1-alias rule applied consistently. A strict stop naming
`0x400C_xxxx` is the evidence that would justify an alias region and a phase of
its own.

**SRAM0's word-only rule is named, not enforced.** `SocBus`'s `RamRegion`
carries `exec` and `writable` and nothing else; the `AccessRule::WordOnly` the
measurement calls for lives on `lp-xt-emu`'s own flat `Memory`, and adding one
to the shared bus is an edit to `lp-emu-esp-common`, which M2 owns and M3 may
only read. So on this machine a guest byte store into SRAM0 **succeeds** where
silicon faults. A known gap, written down where the region is.

`bus_setup::deliberately_unmapped()` is the list a strict stop consults, so a
refusal inside one of these windows says which window it was and why it is
absent, instead of "unmapped".

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

*P2 seam, P3 completion.* `--elf` places the application's `PT_LOAD`s by vaddr
and seeds the entry, `PS_BOOT` and the boot frame described above. That is
**all** it does. The eleven things a direct load does not reproduce
(`m3/notes.md` §3) — the partition table, the flash MMU programming, the ROM
console, `g_rom_flashchip`, the reset cause, eFuse, the APP core's release from
`sw_stall` — are P3's, and every one of them needs a peripheral P2 does not
have.

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

```
--elf <path>            direct-load this image
--boot-mode direct|rom-up
--rom <path>            override the embedded mask ROM
--strict-bus            an access nothing claims is a STOP, not a zero
--time-grade t1         the only grade this machine defines
--timeout <5s|1500ms|900us>   EMULATED time
--wall-timeout <s>      the host-clock safety net; exits 4
--break-at <symbol>     stop at its first instruction
--probe <cycle>:<name>  print the word at a symbol at a guest cycle
--trace <path|->  --trace-block <name>
--seed <n>   --hooks   --map   --help
```

⚠️ `--timeout` is **emulated** time (PD9): no host gate runs on emulated
microseconds, and `--wall-timeout` is the separate wall-clock end.

⚠️ **An unrecognised flag is an error.** The doors P6/P7/P8 add (`--uart0`,
`--uart0-script`, `--control`, `--flash`, `--cache-off-fetch`) are deliberately
*not* stubbed with no-ops, so the phase that adds one is visible in the diff
instead of silently changing what an old command line meant.

Exit codes are the C6's contract: 0 deadline, 2 fault, 3 strict-bus refusal,
4 wall timeout, 5 `--break-at`.

## Bring-up loop

P2 is the first phase that can run it, and P3 runs it in anger.

1. Run `--strict-bus --trace`.
2. Read the **first** stop. `Machine::first_strict_violation` names the
   earliest, and **the earliest strict stop is the root** — an exception after
   it is downstream and tells you nothing.
3. Model that one block, with the pin cited: a PAC reset value, a ROM
   disassembly, a linker-script constant. Never "what the boot needed".
4. Run again.

Without `--strict-bus` the run carries on with unmapped reads answering zero,
which is how far the machine gets before a block is modelled — useful for
scouting, never for a claim.

**Where P2 leaves it.** Both of these are expected stops and the phase's
evidence, not failures:

```text
$ lp-emu-esp32v3 --boot-mode rom-up --strict-bus --timeout 50ms
STRICT BUS STOP
  pc      = 0x4000fdd8 (~_rtc_trigger_sw_system_reset+0x11)
  cycle   = 7 (0 us emulated)
  access  = Read Word at 0x3ff5a000
  where   = inside the declared MMIO window — an UNMODELLED BLOCK
```

`0x3FF5_A000` is **EFUSE**, and the pc is inside
`_ResetHandler_efuse_check_patch` (`0x4000_FDA0` — the tilde on the reported
name means "nearest preceding label", and this ROM's labels are mostly
zero-sized). Seven instructions after the reset vector, the mask ROM reads its
own eFuses.

```text
$ lp-emu-esp32v3 --elf …/fw-esp32v3 --strict-bus --timeout 50ms
STRICT BUS STOP
  pc      = 0x40125775 (esp_hal::soc::xtensa::esp32_init+0x175)
  cycle   = 29 (0 us emulated)
  access  = Write Word at 0x3ff00218
  where   = inside the declared MMIO window — an UNMODELLED BLOCK
```

`0x3FF0_0218` is **DPORT + 0x218**, the first entry of `core_1_intr_map`:
twenty-nine instructions in, `esp_hal::init` starts clearing the APP core's
interrupt map.

## Tests

| Test | What it holds |
|---|---|
| `tests/memmap.rs` | No two declared regions overlap; every `periph::*` base is inside the MMIO window; `dram_seg` is 8 KiB above the ROM's reserve with `RESERVE_DRAM = 0`; the vector table is 1 KiB below the IRAM; SRAM0 is one region containing the bootloader's `0x4007_8000`; the SRAM1 I-bus alias is named and unmapped; the ROM extents match the vendored ELF; 240 cycles to the microsecond |
| `tests/rom_vendoring.rs` | The embedded ROM's sha256, re-derived in-process from `rom::VENDORED_V3_ROM` itself, is the one `SHA256SUMS` records — and the file on disk is still that file |
| `tests/boot.rs` | 39 `PT_LOAD`s, thirteen empty and counted, the ELF-header-mapping segment recognised, four relocated segments placed by vaddr; the ten vector sections at their documented `VECOFS`; at least eight non-alloc sections seeded with real bytes, `.data_xtos_pro` among them; `break 1, 15` matching `lp_xt_inst::encode`; the hook table shipping empty; two hart slots with slot 1 stalled and taking no cycles; `PS_BOOT` after a direct seed; the boot frame's spill target mapped where an unseeded one is not; a seeded hart surviving a real exception; a strict rom-up run stopping inside the MMIO window; snapshot round-trip |

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
