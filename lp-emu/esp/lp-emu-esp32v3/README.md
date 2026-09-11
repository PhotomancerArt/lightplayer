# `lp-emu-esp32v3` — the classic ESP32 (v3) machine

This is where the classic's chip numbers live. `lp-emu-esp-common` supplies
the bus, the peripheral model, the trace and the ELF view and knows nothing
about any chip; `lp-xt-emu` supplies the Xtensa hart and knows nothing about
MMIO. Here they are put together with a memory map, a mask ROM, a reset state
and a run loop, and the result takes a `fw-esp32v3` binary.

It is `lp-emu-esp32c6`'s twin, deliberately: same shape, same module names,
different silicon.

> **M3 P6.** What exists today is the memory map (both peripheral buses),
> the generated register-name tables, the vendored mask ROM, the bus, the
> two-slot machine, the run loop, the snapshot, the direct load, the host's
> side of the wire and the CH340 cable's own socket — and, of the fourteen
> blocks the strict bring-up pass demanded, **eight with behaviour** and six
> still accept-and-remember probes.
>
> **The shipped image boots, on both paths, and answers.** Direct-loaded
> under `--strict-bus` it prints its whole `[INIT]` chain out of UART0's
> FIFO, through a shifter draining at 921,600 baud in emulated time, and onto
> a host stream; mounts `lpfs` out of a modelled flash chip; takes the
> firmware's documented single-core fallback; serves its first frame; and
> idles in `esp_rtos::task::idle_hook`. Started at the mask ROM's reset
> vector instead, it walks the real ROM and the real ESP-IDF
> `v5.1-beta1-378-gea5e0ff298` second-stage bootloader out of a real merged
> image — banner and log line for line against the desk board's own capture —
> and arrives at the same place. **Zero unmapped accesses on either path.**
> A client on the wire gets the wire hello and an answer to its request.
>
> The stop ledger is
> `docs/reports/2026-09-10-esp32v3-strict-boot-inventory.md`.

## The machine

One bus, **two** `XtHart<SocBus>` slots, one schedule.

The layering is `lp-emu-esp32c6/README.md:15-69`'s, and the classic differs in
exactly two ways:

- the hart is `lp_xt_emu::mach::XtHart<SocBus>` rather than
  `lp_riscv_emu::mach::MachineHart<SocBus>`, and
- **`harts` has two slots, and both run** — on one guest clock, a window
  each, the deterministic quantum interleave (M4 P1, plan decision D3).

### Two cores

*Written for someone deciding whether to trust a result.*

**What the interleave is.** `Machine::run_until` hands every core that is
neither held nor parked a window of at most `--core-quantum` cycles (default
`CORE_QUANTUM_DEFAULT = 256`) per iteration, core 0 then core 1, each window
opening at the same guest cycle. Scheduled events fire **between** windows,
never inside one; the interrupt matrix is fed per hart between windows; the
machine's clock is the furthest any hart has got, and a hart that ended a
window early is brought up to it before its next one. A core that is
**held** — by the machine (nothing has started it), by RTC_CNTL's stall key
(`options0.sw_stall_appcpu_c0` + `sw_cpu_stall.sw_stall_appcpu_c1` reading
`0x86`, which is what `with_app_core_stalled` writes around a flash write),
or by DPORT's `appcpu_resetting` / `appcpu_runstall` / `!appcpu_clkgate_en` —
costs nothing and its counters do not move. A core **parked in `waiti`**
costs nothing either: it is not given a window until an interrupt is
*taken*, which is what `waiti` means. When every running core is parked,
guest time jumps to the earliest thing that can wake any of them — the next
scheduled event, the host's next service, a scripted byte, or either hart's
own `CCOMPARE` match. `instructions` in the run report is the sum over both
cores, with each core's count beside it; `core 1:` in the report says which
input is holding it, or `parked(waiti)`.

**How core 1 starts.** The way the part does: `CpuControl::start_app_core`
writes `appcpu_boot_addr`, sets the clock gate, clears the runstall and
pulses `appcpu_resetting`; at the store that completes the release the
machine puts slot 1 at the **architectural reset** — `_ResetVector`,
`PS = 0x1F`, the ROM's `VECBASE`, `CPENABLE = 0xff` — and the mask ROM does
the rest: its reset handler checks `PRID`, its `main`'s APP-core arm spins on
`appcpu_boot_addr` (`0x400076dd`: `memw; l32i; beqz`) and `callx8`'s it.
Nothing is seeded and nothing is hooked; `tests/dual_core.rs` reads the wait
loop's own `DPORT+0x038` reads out of the bus trace. `machine.rs`'s module
docs carry the disassembly.

**What it can show:** a core that never starts; a handler bound into the
wrong core's matrix; a doorbell (`cpu_intr_from_cpu_1`, source 25) that
never arrives; a pusher that never wakes; a flash write that runs without
stalling the other core (D4's cache-off stop is armed per *running* core);
two cores' flash-MMU tables that disagree (ruling R4, below); and — found
while bringing it up — a ROM reset path on core 1 that rewrites memory the
firmware believed was its own.

**What it cannot show, ever:** store-buffer races, cache-coherence windows,
any ordering weaker than "hart 0's store is visible to hart 1's next load".
One `SocBus`, one arena: cross-core visibility here is **stronger than
silicon's**, deliberately, and stated (D3). A firmware bug that needs a weak
memory model to reproduce will not reproduce here. Nor is the interleave
silicon's scheduling: it is a deterministic function of the two instruction
streams, the scripted input and the quantum, and it makes no timing claim.

**The quantum.** A run parameter, never a tuned constant: `--core-quantum
<cycles>`, default 256, printed in the run report (`quantum=256`), in
`core_report()` and carried in the snapshot (a restore adopts the
snapshot's). Two quanta are two interleavings and their cycle counts may
legitimately differ; what may not differ is anything the guest can observe
about itself — the console. One console line is a measurement of interrupt
timing, `[stack] heartbeat: high-water N B`, and does move with the quantum
(and with silicon, by 960 B); `tests/determinism.rs` says so and compares
everything else byte for byte.

**`CPENABLE` resets to `0xff` on this part** (`machine::CPENABLE_RESET`):
measured at the app's first instruction on the desk board, with no writer of
the register anywhere between reset and there — not the ROM, not the
bootloader, not esp-hal. It matters on core 1: esp-hal's `float-save-restore`
interrupt entry saves the FP state unconditionally, and with the ISA's
generic `0` the first doorbell double-faulted.

**Ruling R4 — one window behind both tables.** `SocBus` holds each flash
window in one arena, so the fill serves the PRO core's MMU table and there is
no per-core copy for the APP core's. The IDF bootloader and the direct loader
program both tables from one image, so a fill on which the APP core's entry
disagrees with the PRO core's is a **strict stop** (`FLASH-MMU DIVERGENCE`,
exit 7) naming both entries and the page; `--app-mmu-divergence permit` keeps
P7's warning and serves the PRO core's view.

`Machine::core_stalled` is still the OR of the three inputs P4 and P5 wired;
what changed is that the machine's own hold is released at the DPORT
sequence instead of asserted forever.

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

P2 pinned `a1` to the mask ROM's own PRO-core stack top, `__stack`
(`0x3FFE_3F20`, resolved from the vendored ROM ELF). **P3 pinned the
bootloader's own SP** from the disassembly of both halves of the chain:
`__stack` minus the four `entry` frames between the ROM's `_start` and the
IDF bootloader's `callx8` into the app — `main` 112, `call_start_cpu0` 192,
`bootloader_utility_load_boot_image` 304, `load_image` 64 — is
**`0x3FFE_3C80`**, `loader::BOOTLOADER_SP_AT_APP_ENTRY`, with the chain in
`loader::BOOTLOADER_FRAME_CHAIN` and a test that re-derives it. The
bootloader never sets a stack pointer of its own; it runs on the ROM's. What
the four save-area words hold on silicon is P7's ROM-up run to measure.
`BootFrame` stays a builder parameter so that measurement has somewhere to
land.

### Time

`TimeGrade` is **t1 only** in M3: cycles are instructions, and
`micros = cycles / 240` (`memmap::CPU_HZ`). `--time-grade t2` is refused with a
message naming the calibration that does not exist: there is no measured
Xtensa per-instruction-class table in this repo, and six months from now an
invented one would be indistinguishable from a measured one. The classic does
have a better calibration source than the C6 ever had — `CCOUNT` is CPU cycles
at 240 MHz — but that is an M5/M7 opportunity, not an M3 one.

### Two decisions the bus makes, and one gap it has

**RTC fast memory's instruction-bus view is not mapped** (ruling DD36).
`memory.x:51` and `:54` put the same 8 KiB block behind `0x400C_0000` (I)
and `0x3FF8_0000` (D). `SocBus` cannot express that: its regions are
asserted non-overlapping and its bytes live in one flat arena keyed on
`address - arena_base`, so two regions are two independent stores and a
write through one is invisible through the other. So the D-bus view is
mapped and the I-bus view is **named and unmapped** — the SRAM1-alias rule
applied consistently, and `memmap.rs`'s `RAM_SPANS` comment says so since
P3. A strict stop naming `0x400C_xxxx` is the evidence that would justify an
alias region and a phase of its own.

**SRAM0 takes aligned 32-bit data access and nothing else** (ruling DD37).
`fw-esp32v3`'s `test_sram0_exec` rig measured that on the desk board: a byte
store at `0x4008_8000` raised `LoadStoreError` / EXCCAUSE 3 / EXCVADDR = that
address, while 16,384 aligned word stores across the same span read back.
P2 and P3 could only *name* the rule — `RamRegion` carried `exec` and
`writable` and nothing else — and P4 landed `AccessRule` on the shared bus,
so it is now enforced: a byte, half-word or misaligned **data** access into
`0x4007_0000..0x400A_0000` is a fault with the cause the rig measured.
Instruction fetch is untouched, which is what the memory is for, and the host
side (a loader, a ROM seed, a cache fill) still writes bytes.

**Every block from `0x3FF4_0000` up answers at its AHB address too** (ruling
DD38). The classic has two peripheral buses, and the mask ROM reaches blocks
through `0x6000_0000 + (base - 0x3FF4_0000)` that the PAC names only on the
DPORT side (P3 §3.2). Registering a block twice would be two states — the
SRAM1-alias mistake in MMIO form — so the shared bus grew
`add_peripheral_alias`: **one state, two decodes**, with the trace flagging
which door an access came through. `lp-emu-esp32v3 --map` prints the mirror.
The direction is the evidence's: a PAC-named DPORT base gets an AHB alias,
and `I2C_ANA_MST`, whose only cited address is the AHB one, gets no
DPORT-side twin.

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
0x3FF0_0000 +0x08_0000  MMIO (DPORT)  the peripheral window
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
0x6000_0000 +0x04_0000  MMIO (AHB)    the same peripherals, second bus (P3)
```

**The classic has two peripheral buses.** P3's fifth strict stop was the
mask ROM's `rom_chip_i2c_writeReg` writing `0x6000_E010` — outside every
window the map declared. The ROM computes the analog I2C master's address
as `(0x1800_3800 + host_id) << 2`, its `.text` loads forty-odd literals in
`0x6000_0000..0x6002_2000` that line up with PAC-named DPORT blocks
(`SENS`, `NRX`/`BB`, `FLASH_ENCRYPTION`) offset by `0x3FF4_0000 −
0x6000_0000`, and the PAC's `RNG` at `0x6003_5000` — which P1 had excluded
as an SVD leak — is WDEV's AHB address, its `data` at `+0x144` the
classic's `WDEV_RND_REG`. So `AHB = 0x6000_0000 + (DPORT − 0x3FF4_0000)`,
declared as the second MMIO window (`memmap::MMIO_AHB_BASE`, with the
evidence). What the machine does about the mirror: a block is registered at
**one** of its two addresses — the DPORT one where the PAC names it, the AHB
one where only the ROM does — and an access through the other is a strict
stop that names the window and the twin address, because `SocBus` cannot
put one state behind two bases (the SRAM1-alias rule, DD24/DD36, in MMIO
form). A guest reaching a PAC-named block through AHB is the evidence for a
forwarding view or a bus feature; P3 reports the question.

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

`src/loader.rs`'s module doc is the documentation; this section points at
it. `--elf` places the application's `PT_LOAD`s by vaddr (recording the one
whose `paddr` differs — `.rtc_fast.persistent`), seeds the ROM's flash chip
description with the chip size (through the ROM's own symbol for it,
`spi_w25q16`; the vendored ELF has no `g_rom_flashchip` — that is ESP-IDF's
linker alias, and the notes' claim is corrected in the loader), and seeds
the entry, `PS_BOOT` and the bootloader's frame described above.

**`.data` is a self-copy on the classic** — verified on the image
(`_sidata == _data_start == 0x3ffb0000`): the app's `Reset` runs its copy
loop, unlike the C6's, and moves every word onto itself, so the loader
placing the DRAM segment at its vaddr is what makes the copy a no-op.

**The eleven things a direct load does not reproduce**, numbered in the
loader's module doc so P7's cross-check can cite them: (1) no partition
table or image validation; (2) the flash MMU is programmed by the **loader**
— since P7 `stage_image_in_flash` puts the image's flash-resident pages into
`factory` and maps them, so the windows really are served through the table,
but the offsets are the loader's arithmetic and not an `esptool` image's
layout; (3) the ROM console never initialised; (4) no ROM banner, no
bootloader log; (5) no early RNG entropy; (6) eFuse asserted, not read; (7)
the reset cause asserted as POWERON; (8) `.data` placed rather than copied;
(9) core 1 stalled by assertion, not by the ROM's `sw_stall` path; (10)
`chip_size` written by the loader in place of
`esp_rom_spiflash_config_param`; (11) the cache left enabled, as the
bootloader's `Cache_Read_Enable` leaves it — the state D4's stop (P4) is
defined against.

**What is behind SPI1 matters to what the boot prints.** A direct load with
no `--flash` gets a *blank* part: there is no partition table at `0x8000`, so
the firmware's `lpfs` lookup fails and it says so and falls back to its
memory filesystem. With the merged image behind it (`--flash-copy`, or
`--merged`) the mount succeeds and the chain ends `[INIT] flash filesystem
mounted`. Both readings are pinned in `tests/boot.rs`, each as a whole byte
stream with its own sha.

`just test-emu-esp32v3-boot` builds the shipped image and runs the
direct-load tests against the file it built (`LP_EMU_ESP32V3_ELF`; see
`src/test_support.rs` for why the conventional target path is never trusted
from inside a test).

## Booting from the reset vector

```bash
just build-fw-esp32v3
espflash save-image --chip esp32 --merge \
    --partition-table lp-fw/fw-esp32v3/partitions.csv --flash-size 4mb \
    target/xtensa-esp32-none-elf/release-esp32v3/fw-esp32v3 chip.bin
cargo run -p lp-emu-esp32v3 --release -- \
    --boot-mode rom-up --merged chip.bin --strict-bus --timeout 2s --uart0 -
```

The hart starts at `0x4000_0400` with the architectural reset state and the
machine seeds **nothing**: the flash chip holds a whole 4 MiB image and every
step after that is real. The mask ROM checks its own fuses, reads the
strapping pins, attaches the flash, programs the MMU, reads the second-stage
bootloader out of the chip and jumps to it; the bootloader reads the partition
table, verifies and hashes the app, and maps it.

**Nothing is vendored** (DD25). `espflash save-image --chip esp32 --merge`
bundles the exact ESP-IDF `v5.1-beta1-378-gea5e0ff298-dirt` second-stage
bootloader the desk board runs (`../bench.md`), so the merged image *is* the
provenance; a checked-in copy plus a sidecar would be a second one that could
drift. `tests/rom_up_boot.rs` asserts the version string, the compile time and
the multicore banner it finds **inside the image it was handed**. ⚠️ Never
hand-write a bootloader stand-in: the boot log is the real bootloader's output
or it is fiction.

### What comes out, and what it is compared against

Twenty-six lines, and they fall into three kinds:

| lines | compared against | why |
|---|---|---|
| the eleven ROM banner lines (`ets Jul 29 2019 12:21:46` … `entry 0x4008064c`) | **literally**, against L0's own capture | the mask ROM is a fixed binary and the values are this machine's inputs (the reset cause, the strapping pins) or the image's header |
| the bootloader's fifteen (version, compile time, `Multicore bootloader`, `chip revision: v3.1`, SPI 40MHz/DIO/4MB, the RNG entropy line, the four partition rows) | **literally**, against a table in the test | espflash bundles a fixed binary, so these are the same on any host and for any build of the application |
| the seven `esp_image: segment N:` lines | against the **merged image**, parsed independently by `image.rs` | gating those on a transcript would gate on the linker: a different build has different segment sizes, and the desk board runs a different commit (ruling R7) |

The `I (NNN)` millisecond stamps are **masked**, and that is the only field
that is. They are `CCOUNT / (g_ticks_per_us * 1000)` — milliseconds of CPU
time — and this machine's time base is grade `t1`, cycles counted as
instructions, so the numbers are a count of the bootloader's own instructions
rather than a clock. There is no calibration in this repository that would
make them silicon's.

### Where it used to stop, and what happened when it did not

Until M1 P6 the walk ended **one instruction short** of `Loaded app from
partition at offset 0x10000`, in the bootloader's own
`esp_cpu_dbgr_is_attached()`:

```text
4007a523:  l32r  a14, (0x0010200c)    ; XDM_OCD_DCR_SET
4007a526:  rer   a14, a14             ; ← lp-xt-inst did not decode this
```

`rer` reads an *external* register over the OCD bus, and the only thing
either boot path uses it for is "is a debugger attached?" — whose answer here
is a flat **no**. The direct load met the same instruction from the other
side, at `0x4010_01bd`, through `esp_hal::debugger::debugger_connected()`
inside `CpuControl::start_app_core`. It was pinned by pc and by word rather
than fixed from an M3 branch, because `rer`/`wer` were absent from
`lp-xt-inst`'s `Inst` altogether and that crate is the Xtensa shader
backend's too. **M1 P6 landed them.**

Both walls fell at once, and three things behind them turned out to be
waiting:

- through M3 the direct load took the firmware's documented single-core
  fallback (`main.rs:838-845`, Q5) and printed `[INIT] APP core unavailable;
  RMT ISR on PRO core (single-core semantics)` where silicon prints `[INIT]
  RMT ISR on APP core`. Heap region 3 is added in **both** arms, so the heap
  arithmetic is the same either way — which is why G2 compares memory-class
  fields and not log text. **M4 P1 releases core 1** ("Two cores" above), so
  this machine now prints silicon's line, and G2's memory half was re-pinned
  with core 1 running. It did not get there on the first try: the release was
  first modelled as a reset *through the mask ROM*, whose unpack and bss
  tables rewrote `0x3ffe0440..0x3ffe1320` — memory the firmware has already
  given to its allocator as heap region 0 — and the shipped image died ~30k
  cycles later in `LpFs::read_file`. The bench refused that model
  (`changed=0` on silicon, twice) and it was corrected; the entry is
  `docs/defects/2026-09-10-the-emulator-ran-the-rom-reset-path-on-the-app-core.md`,
  **fixed**, and it is worth reading before touching the release path;
- the ROM-up walk runs on through `Loaded app from partition`, `Disabling RNG
  early entropy source` and the `E boot: Image contains multiple DROM
  segments` line that the desk board prints on every boot of this image, and
  hands over to the application;
- the **cross-check** (`rom_up_and_direct_load_agree_on_what_the_app_sees`)
  runs, and it is the subject of the next section.

### What the two paths agree on at the application's entry

2,189,961 bytes of the application's own image, byte for byte, in RAM and
through both flash windows; `PS`; `a1`; `VECBASE`; the four save-area words;
and all 256 flash-MMU entries. Four of those were **placeholders until this
comparison measured them**, and each is now a named constant with the run
that produced it beside it:

| what | was | is | where |
|---|---|---|---|
| `PS.OWB` at the app's entry | 0 | **7** | `machine::BOOTLOADER_OWB` |
| the save area at `[a1-16, a1)` | `[0, sp, 0, 0]`, a coherent invented frame | `[0, 0x3ffe3ca0, 0x3ffe3c15, 0x3ffe3cc0]` | `loader::BOOTLOADER_SAVE_AREA` |
| an **unmapped** flash-MMU entry | `0` — which, with no valid bit, *is* a mapping of flash page 0 | **`0x100`**, what the ROM's own `mmu_init` leaves | `cache::MMU_UNMAPPED` |
| the breakpoint that stops both paths there | a `break` planted before the run | armed when the app's own bytes arrive | `Machine::break_at_address_when` |

⚠️ **Thirty-two bytes are excluded, and the ROM-up side is the one that is
right.** The application ELF's DROM program header is one contiguous
`0x3f400020..0x3f447410`; `espflash` splits the same bytes into image
segments with an eight-byte header in front of each, so the flash page behind
the DROM window carries those headers — and the sixteen bytes of the DRAM
segment between them — where the ELF carries padding. Measured at
`0x3f400122`: ROM-up reads the flash's own bytes, the direct load reads zeros
because its loader placed the ELF's contiguous view. Nothing reads them on
either path; the comparison is over **what the bootloader placed**, the
excluded count is printed, and making the direct loader place flash pages
rather than ELF segments in the two windows is the fix somebody else gets to
make.

⚠️ **A hook cannot be planted in IRAM ahead of a ROM-up boot**, which is what
`break_at_address_when` exists for. Two reasons at once: the bootloader loads
its own segment over `0x4008_0404..` and then the app's over `0x4008_0000..`,
so the patch is gone twice over before the pc arrives; and the bootloader's
own code occupies the same addresses at **different instruction boundaries**,
so a patch that simply re-armed itself planted three bytes through the middle
of one of them and the run died on an undecodable word a few bytes later.
Separately, `rom::read_three` used to read the displaced instruction a byte at
a time, which SRAM0's measured word-only rule (DD37) refuses — so a
`--break-at` anywhere in IRAM had been failing silently on **both** paths.

### The two findings a ROM-up boot is the only way to make

Both are in the commit history and both were invisible to a direct load:

1. **`seed_data_image` is not optional on the classic.** The reset vector's
   `unpcopy` (`0x4000_0501`) copies each `.data_*` section from a **source**
   address past the end of `.text`. P2 placed the destinations and concluded
   no source image was needed; a ROM-up boot copied zeros over every one of
   them, `g_ticks_per_us` went 13 → 0, and the bootloader's log timestamp
   divided by zero four million cycles later. `rom::seed_data_image` now
   places the ROM's own copy at the addresses the ROM's own table names.
2. **A write that does not change an MMU entry still maps the page.** The
   classic's entry carries no valid bit, so `cache_flash_mmu_set` storing `0`
   over `mmu_init`'s `0` *is* a mapping of flash page 0 — and a fill that
   skipped it left the ROM reading its own bootloader header out of an
   unfilled window, printing `invalid header: 0x00000000` for ever.

## The CH340 cable

**On the classic the port is a bridge chip on the board, not a peripheral
inside the SoC.** That single sentence is the whole difference from the C6,
and it changes what four verbs mean.

The C6's `USB_DEVICE` can see the host: a client attaching moves chip state a
guest can read. A CH340K cannot be seen from inside the ESP32 at all. There
is no register anywhere on this part that reports a cable, a port, or a modem
line. So:

1. **Opening or closing the port moves no chip state.** A byte client
   connecting to `--uart0 tcp:` is a program that opened a tty on a bridge
   chip; the SoC does not know it happened, and there is no coupling rule to
   write. (The C6 has one: a client on its byte socket *is* an application
   opening the port. That rule does not exist here, and its absence is the
   finding, not an omission.)
2. **What resets the chip is the auto-reset circuit on the carrier board**,
   driven by the two modem lines.
3. **The truth table is the board's, not the chip's**, and half of it is
   measured and half is documented.

### The circuit

esptool documents it as *Classic reset*: two transistors, wired so that
neither line **alone** can hold both strap lines.

```text
    EN  low  ⟺  RTS asserted AND DTR not asserted
    IO0 low  ⟺  DTR asserted AND RTS not asserted
    both asserted → both EN and IO0 stay high
```

| dtr | rts | EN | IO0 | effect | grade |
|-----|-----|----|-----|--------|-------|
| 0 | 0 | 1 | 1 | run | **measured** — L0 drove `TIOCMSET 0` and the board ran |
| 0 | 1 | 0 | 1 | RESET held | **measured** — L0 drove `TIOCMSET 0x4` (RTS only), EN went low and the board reset |
| 1 | 0 | 1 | 0 | IO0 low: the download strap | `documented` |
| 1 | 1 | 1 | 1 | run (the circuit's whole point) | `documented` |

`../bench.md` is L0's measurement and it covers **two rows and no more**.
Rows 3 and 4 are esptool's circuit plus this repo's own sequences
(`spikes/serial-lab/index.html:341-357`), and they stay `documented` until L1
captures a download-mode entry. A model that happens to work is not a
measurement.

### The verbs, and why the reboot is an edge

```text
reset          = {dtr:0,rts:1} → hold → {rts:0}
download-mode  = {dtr:0,rts:1} → {dtr:1,rts:0} → {dtr:0}
```

Both are shorthands for the line sequence, and both are the sequences the
repo's own serial lab uses. **The reboot happens on the release, not on the
assert**: EN low holds the chip in reset, and EN going high is the chip
starting, latching IO0's level at that instant as the strap. Writing it as
edges rather than as verbs is what makes `signals dtr=0 rts=1` followed by
`signals dtr=0 rts=0` exactly one reboot into the application, and the
download dance exactly one reboot with IO0 low — the circuit, not a special
case per verb, and the third step of the download dance harmless because the
chip has already started.

`--reboot-on-reset` decides what a release does. Off (the default for a plain
run) it ends the run with `RESET` and names the strap; on, the machine goes
back to the state it was built in. A reboot costs a whole copy of guest
memory taken at build time, which is why it is not the default.

**The reset cause does not change across a cable reset, and that is right.**
An EN-pin reset on this part is a *chip* reset — it resets the RTC sub-system
too — and the classic has no code for one. esp-hal's own `SocResetReason`
table for esp32 (`third_party/esp-hal/src/rtc_cntl/rtc/esp32.rs:15-50`) runs
`ChipPowerOn = 0x01`, `CoreSw = 0x03`, … `SysRtcWdt = 0x10` and has no
external-reset variant at all, so `RTC_CNTL.reset_state` reads
`POWERON_RESET` after a cable reset exactly as it does after a power-on. This
is where the classic differs from the C6's `USB_UART_HPSYS`, and it differs
by having **no** code rather than a different one.

The console keeps its bytes across a reboot: a restore would put back the
empty log the machine was built with, and a boot log that lost everything
before the reset would be a worse record than one with both boots in it.

### The protocol

One command per line, one reply per command, `\n`-terminated, on the socket
`--control tcp:<addr>` listens on. `--control-script <path>` is the
deterministic twin: the same verbs with the times in the file.

| verb | what it does here |
|---|---|
| `attach` / `detach` | the cable goes in or comes out. **Host bookkeeping only**; `detach` also lets both lines go slack, which on this circuit is "run" |
| `open` / `close` | an application opened or closed the tty. **Host bookkeeping only** |
| `dtr 0\|1`, `rts 0\|1` | one line |
| `signals dtr=… rts=…` | both lines in one write — what whole-status `TIOCMSET` does, and the only thing the WCH macOS driver honours |
| `reset` | the classic-reset dance above |
| `download-mode` | the download dance above |
| `state` | `ok state cyc=… us=… cable=… port=… dtr=… rts=… en=… io0=… strap=… reboots=…` |
| `wait <ms>` | script only: shift every later line |

The C6's `usb-write`, `pin` and `pins` are **not** verbs here, and the error
says so by name: there is no USB endpoint to write into (a host on this board
sends bytes down the wire, which is `--uart0-script` or the byte socket), and
the pad fabric has no *waveform* to drive a pin against until **M4**. The
fabric itself exists — see [Pads](#pads) — and the verbs that reach into it
arrive with the RMT channels that make a pin observation mean something.

### A host-side fact that belongs beside the cable

The WCH macOS driver **silently ignores** the single-bit
`TIOCMBIS`/`TIOCMBIC` ioctls behind pyserial's and serialport's `.dtr`/`.rts`
setters, and honours only whole-status `TIOCMSET`. Verified on hardware and
recorded in product code at
`lp-app/lpa-client/src/stream/serialport_stream.rs:51-60`, which also notes
that espflash carries `UnixTightReset` for the same reason. That constrains
the lab script and `scripts/emu/`, **not** this emulator — written down here
so nobody re-derives it, and so that a script driving the real board and one
driving this socket are known to differ.

## Pads

Forty of them, in two 32-bit banks, over the bus's own signal fabric
(`lp_emu_esp_common::pins`). The fabric is where the routing lives because a
peripheral never sees another peripheral: `GPIO` writes the routing, `IO_MUX`
writes each pad's input enable, and an output block drives its signal without
ever learning who is listening.

Three of the classic's numbers are **not** the C6's, and every one of them
would have failed quietly:

| | classic | C6 |
|---|---|---|
| `func_out_sel_cfg.out_sel` | bits 0:8, and **256** means "follow `GPIO_OUT[n]`" | bits 0:7, 128 |
| input signals | **256** (`func0_in_sel_cfg` … `func255_in_sel_cfg` at `+0x130`) | 128 at `+0x154` |
| `func_in_sel_cfg` constants | **48** low, **56** high | 0x3c low, 0x38 high |
| `IO_MUX.mcu_sel`'s GPIO function | **2** | 1 |

The C6's `OUT_SEL_GPIO` is an ordinary signal number here, and nothing drives
it — so a pad routed with the wrong constant looks routed and reads low,
which is exactly what a plain output pad is *supposed* to look like before
anything writes `out`. `the_gpio_selector_is_256_and_128_is_an_ordinary_signal`
is the test that says so.

⚠️ **`IO_MUX`'s pad registers are not in pad order.** `+0x004` is `gpio36`,
`+0x044` is `gpio0`, `+0x088` is `gpio1`: the block is laid out in the order
the pads leave the package. The C6's `GPIO0 + 4 * pad` arithmetic would push
`fun_ie` up to thirty-five pads away from the one the driver meant, and every
one of those writes would still look plausible. `io_mux::PAD_OF_OFFSET` is
the map, transcribed from the **generated** table's own register names and
walked against them by a test so the two cannot drift. Thirty-six entries:
this part has no GPIO 28..31.

And two the classic has that the C6 does not: `out1`/`enable1`/`in1`/
`status1` carry pads 32..39 as live registers rather than padding, and
`pcpu_int`/`acpu_int` are one interrupt output per core. `int_ena`'s bit order
is taken from `esp-hal-1.1.1`'s `gpio_intr_enable` (bit 0 = APP, bit 2 = PRO,
so `pin[n]` bits 13 and 15) rather than from the PAC's own prose, which skips
a bit and runs past the width of the field it describes.

### What the pin class is trusted for: a waveform, not yet a frame

The routing is modelled. `IO_MUX`'s `fun_ie` reaches the fabric. `GPIO.enable`
decides which routed pads drive. Through all of M3 **no waveform was produced
and none was decoded** — `RMT` was an accept block, so no `RMT_SIG_n` was
ever driven and every routed peripheral pad sat low.

**M4 P2 changed the first half of that.** The RMT view's symbol pump drives
`RMT_SIG_0 + n` at every pulse edge, so a pad whose `func_out_sel_cfg` names
that signal now carries the waveform, and the machine drains the fabric's
edges at every window boundary and hands them to `GPIO` — the same stream M4
P3 hangs the strip decoders and the `.pins.jsonl` log off, which is what
guarantees a decoder, a pin log and the GPIO input latch can never disagree
about what was on the wire.

`Gpio::peripheral_driven_pads` lists the pads that are output-enabled **and**
routed to a peripheral signal rather than to `GPIO_OUT`. M3's gate asserted it
was empty; from P2 it is the list a strip decoder should be watching, and
`tests/rmt_registers.rs` asserts a routed pad carries both edges of every bit
of a whole WS2812 frame. **Nothing decodes it yet** — that is P3, and no
frame claim is made here.

Not modelled, and each of these is an electrical fact a logic analyser on the
pin header would not show you either: drive strength, the *value* of a pull-up
or pull-down (an undriven pad reads low, not "pulled high"), open-drain, pad
filters, the input synchroniser, and analog anything.

## RMT on the classic

*M4 P2. `periph/rmt.rs`; every offset out of `regs::RMT` (62 registers, esp32
PAC 0.40.2), every bit position out of that PAC's field docs.*

Eight channels, each TX-or-RX — there is no fixed split on this part — over a
**512-word RAM at `0x3FF5_6800`** (`+0x800`, eight 64-word blocks). The clock
is APB at 80 MHz with a per-channel `div_cnt`, so one tick at `div_cnt = 1` is
12.5 ns and, at a 240 MHz CPU, exactly three cycles; the model computes that
from `memmap::CPU_HZ` and `APB_HZ` rather than writing a 3. `int_st` is
`int_raw & int_ena` and the line goes out on **source 47**.

Whether the block transmits is the driver's business, and the shipped image
does not until a project's output opens: a boot configures four two-block
slots (the DOM-Z-102's five-wire plan, `[2, 0, 2, 0, 2, 0, 2, 0]`), arms their
interrupts, clears all 512 RAM words and stops there.

### The five quirks, three of them inverted relative to the C6

A reader who knows `lp-emu-esp32c6`'s block is the reader most likely to be
wrong here, so those three come first. Borrowing the C6's **tick arithmetic**
is right — it is arithmetic, not layout. Borrowing its semantics is a frame
that truncates and a model that passes every register test anyway.

1. **`tx_lim` is a repeating count of words *sent*, and it re-arms itself.**
   PAC `rmt/ch_tx_lim.rs`: *“When channel0 sends more than reg_rmt_tx_lim_ch0
   datas then channel0 produce the relative interrupt.”* One programmed value
   fires every `tx_lim` words for the whole frame. **The C6's names a word
   offset in the window.** The firmware knows, and clamps the driver core's
   alternating half/window request down to a fixed period because of it
   (`v3_rmt.rs`, with the measurement that proved it: `guard_trips` exactly
   equal to `frames` on every channel whose frame outgrew one window).
2. **`apb_conf.mem_tx_wrap_en` is global** — one bit (bit 1) for all eight
   channels, not a per-channel bit in `chNconf0`. Without it the transmitter
   runs off the end of the window instead of wrapping onto the half the driver
   has just refilled, and ping-pong refill does not work at all. The boot
   trace has `init_tx` setting it: `apb_conf 0x00000001 -> 0x00000003`.
3. **There is no `conf_update`.** esp-hal's `update()` is a literal no-op on
   this chip, and the write that carries `tx_start` starts the channel then
   and there. The C6 model's “pulses take effect at `conf_update`” rule is not
   carried over.
4. **There is no `tx_stop` bit.** A stop is the channel's whole window filled
   with end markers; the transmitter halts at the next word boundary and
   raises `tx_end`.
5. **The read pointer is absolute** — ten bits over all 512 words, so a
   channel's window begins at `64 × first_block` and the driver subtracts it.

### ⚠️ Where the read pointer actually lives

The PAC puts `MEM_WADDR_EX` at **bits 0:9** and `MEM_RADDR_EX` at **bits
12:21** (`esp32-0.40.2/src/rmt/chstatus.rs`) — the opposite of the layout M4's
planning notes carried, and the SVD's own field *descriptions* are swapped on
top of that (`MEM_WADDR_EX` is documented as *“The current memory read
address”*).

What settles it is that `v3_rmt::read_pos` and esp-hal's `hw_offset` both read
the pointer through the **accessor named `mem_raddr_ex`**, which is bits
12:21. That is where this view publishes it. A model that had followed the
notes would have handed `read_pos` a constant zero, and every refill would
have filled the half the transmitter was standing in — a frame that
truncates, from a block whose every register test passes.

### Two more the firmware's cross-core design depends on

- `int_clr` is **write-only and W1C**: each write clears exactly the bits it
  names, which is what makes it race-free with the RMT ISR on core 1 and
  thread context on core 0 both writing it.
- `mem_owner` (`chNconf1` bit 5) is cleared by `start_tx` for the channel
  **and every extra block its window extends into**. It is recorded and never
  enforced, and `chNstatus.mem_owner_err` never rises: this model has one RAM
  and no arbiter.

### What the bits are, as the shipped driver writes them

The interrupt bits are the thing a reader gets wrong first, so here they are
against a real boot. `ch<N>_tx_end` is bit `3N`, `ch<N>_rx_end` bit `3N+1`,
`ch<N>_err` bit `3N+2` (a **combined** TX/RX error — there is no separate
`tx_err`), and `ch<N>_tx_thr_event` bit `24+N`. All 32 bits are defined, so
nothing is masked away. The driver arming its four slots walks `int_ena`
through exactly that layout:

```text
cyc=3513340 W4 RMT+0x0a8 int_ena = 0x01000005   ch0: tx_end | err | tx_thr_event
cyc=3515483 W4 RMT+0x0a8 int_ena = 0x05000145   + ch2
cyc=3517629 W4 RMT+0x0a8 int_ena = 0x15005145   + ch4
cyc=3519776 W4 RMT+0x0a8 int_ena = 0x55145145   + ch6
```

### The engine, and what is live

One word is two pulses of `(dur1, level1)` / `(dur2, level2)`, `dur` in
channel ticks, level in bits 15 and 31. The next word is due at
`start_cycle + cycles_for(ticks_since_start)` — integer arithmetic over the
**absolute** tick count since `tx_start`, never accumulated per word and never
taken from the dispatch cycle, so nothing rounds across a 3,600-word frame.
An end marker (a zero first duration) ends the transmission before it; a zero
second half emits half one and then ends it.

The RAM is **live**: the engine reads `ram[raddr]` when it fetches, so a
refill overwrites what the consumer has not fetched yet. That is what a
single-ported RAM does and it is what the driver's guard word exists for.

### The refill telemetry

For every `tx_thr_event` the block counts the words the transmitter consumes
before the guest's next `chN_tx_lim` write (the **entry** delay) and then
before the last RAM write of that refill (the **fill**), in the same units
`lp-ws281x` measures them in. Nine buckets, eighths of a half-window, the same
edges the guest's `[WS281X]` line uses, so the two can be printed side by side
and read as one shape. The CLI prints it under the run summary.

**Reported, never gated** (D13/PD9), and the reason is in the numbers rather
than in the policy: the emulated ISR path is RAM-resident by construction and
this machine has no flash-miss cost, so the entry half is a floor rather than
a prediction of silicon's 20–29 words. What it is good for is the shape — a
fill that grows, or a bucket that starts landing at “≥ half”, is the model or
the driver getting slower at the deadline.

### What is not here

- **RX.** The classic firmware never sets `rx_en`. The bits are accepted and
  remembered; `rx_en` on a channel is a `log::warn!` naming it, not a model.
- **Carrier** modulation: `chNconf0.carrier_en` and `chNcarrier_duty` are
  accepted with one note. The driver turns it off.
- **The APB FIFO** (`chNdata`, `chNaddr`): `apb_conf.apb_fifo_mask` is what
  esp-hal's `Rmt::new` sets, and direct RAM access is the only path this
  firmware uses.
- **`ref_always_on = 0` (REF_TICK).** The PAC's reset for `chNconf1` leaves
  bit 17 clear, so an *unconfigured* channel selects `clk_ref`; esp-hal's
  `configure_clock` writes it to 1 for every channel before anything
  transmits. A channel started on REF_TICK is **refused** — a `log::warn!`
  and no waveform — rather than clocked at an invented rate, the way the C6
  refuses `sclk_sel = 2`.
- **The decoder and any frame claim**: M4 P3 and P4.

### Grades

`int_raw`, `int_st`, `int_ena`, `int_clr`, `ch*conf0`, `ch*conf1`,
`ch*status`, `ch*_tx_lim` and `apb_conf` are **`documented`** — the PAC's bit
map read out loud. `ch*data`, `ch*addr`, `ch*carrier_duty` and `date` are
**`modeled`**: accept-and-remember at the PAC's reset. Nothing is `measured`,
and nothing will be from a waveform — a frame off a pad is not a register's
bit map.

### For M8, not for now

Three things in `periph/rmt.rs` are the C6's file's too and would survive an
extraction into `lp-emu-esp-common` once a third chip asks: the tick
arithmetic over absolute ticks, the pulse and word observation logs, and
`RefillStats` with its buckets. **Nothing is extracted** (decision D2, M4
ruling R8) — the layouts differ in every register and three of the semantics
are inverted, and one running example is not a generalisation.

## The pad

*M4 P3. The decoder, the two sinks and the name table.*

A pad becomes **observed** the moment the guest routes it — the instant a
write to `func_out_sel_cfg[n]` changes `Fabric::route_epoch`, the machine
gives that pad a `Ws281xDecoder` and starts feeding it. Nothing has to be
configured on the command line for that to happen, and a pad nothing routed
is never decoded, never logged and never in `routed_pads()`.

The decoders are fed from **`Machine::drain_pins`**, once per *window*, after
both cores have had theirs and before the matrix feed. There is exactly one
drain and everything that watches a pad reads the same stream out of it: the
GPIO block's input latch, the strip decoders, and the pin log. That is what
makes it impossible for the three to disagree about what was on the wire.

> ⚠️ **Two cores, one fabric.** The edges are drained once per window, not
> once per core, and the cycle a decoder reads is the **edge's own `at`** —
> stamped by whoever drove the signal, not by the drain. The RMT emits both
> halves of a word at the fetch, so an edge can be stamped slightly ahead of
> the boundary it is drained at; that is a timestamp the decoder reads, never
> a reordering. The interleave therefore cannot move a waveform, and
> `tests/pin_frames.rs` pins that by decoding the same frame at two
> `--core-quantum` values and comparing the dumps byte for byte.

### 240 MHz, as a parameter

`cpu_hz` is a **parameter** of `Ws281xDecoder`, and this machine passes
`memmap::CPU_HZ` — **240 MHz**, against the C6's 160. One WS2812 bit is a
different number of cycles on the two chips and every threshold the decoder
applies follows from `cpu_hz`, so a literal anywhere on this path would
silently misread one of them. The tolerance is the datasheet's ±150 ns and is
**not a knob**: a pulse outside it is a finding about the transmitter, and a
decoder that shrugged at one would be worth nothing as an oracle.

### The three readings, and what they are not

| reading | where it comes from |
|---|---|
| the words the engine fetched | `--rmt-logs`, `Machine::rmt_words(ch)` |
| the pulses it drove | `--rmt-logs`, `Machine::rmt_pulses(ch)` |
| the bytes off the pad | the decoders, `Machine::frames(pad)` / `--dump-frames` |

> **All three are ours.** The decoder is this repository's, the fabric is this
> repository's, and the RMT model is this repository's — so a frame read off
> the pad and a frame the guest describes are **two readings of one machine**,
> not a measurement. What they buy is that a bug has to be in the *same* place
> in three independent code paths to hide. A silicon twin of the transcript is
> M5's, and it is the only thing that turns any of this into a measurement.

### A frame three ways

*M4 P4. `walks/shader-oracle.script`, `tests/shader_oracle_pin.rs`,
`scripts/emu/m4-walk-esp32v3.sh` (`just walk-esp32v3-emu-frame`).*

The three readings above are all of the *machine*. There is a second three,
and this one reaches outside it: the same 64-pixel frame, read off the
firmware, off the pad, and off a host that never saw either.

| reading | source | shape |
|---|---|---|
| (a) the firmware's own | `[OUT] dump frame=… rgb=…` on the UART0 console (the **`frame-dump` image**) | lowercase hex, RGB **as the driver's `write(data)` received it** |
| (b) off the pad | `frames(18)` → `unpermute(&f.wire, ColorOrder::Grb)` | the wire carries GRB; the driver's input is RGB |
| (c) the host oracle | `cargo test -p lpa-server --test shader_oracle_frame -- --nocapture` → `[ORACLE] rgb=` and `[ORACLE-RV32] rgb=` | pinned in `tests/shader_oracle_pin.rs` with the command that produced it |

`projects/test/shader-oracle` renders the same 64 pixels every frame — no
clock, no interpolation, no dithering, no LUT — so all three are the same 384
characters or something is wrong. Measured in this tree, both host engines
agree byte for byte: `crc=0x55772254`, `[ORACLE-DIFF] 0 differing bytes of
192`.

**The project is retargeted, never forked.** `output.json` names
`ws281x:local:D10`, the XIAO S3's pad; the DOM-Z-102 has no D10 — its labels
are IO18 / IO16 / IO14 / IO2 (the four fused DATA terminals) and IO13 (the
spare screw terminal). An output node whose endpoint the board does not have
never opens, the device renders nothing, and a walk then reports a pixel
mismatch that is really a mis-addressed pin. `scripts/m4-hardware-walk.sh`'s
`prepare_project` rewrites the label into a **scratch copy** and fails loudly
if the substitution matched nothing; `scripts/emu/m4-walk-esp32v3.sh` does the
same, by the same mechanism, *so both chips render provably identical pixels:
the endpoint chooses a wire, never a colour.*

**The dumped frame is not the first lit frame.** `frame_dump.rs`'s
`LIT_DUMP_DELAY_FRAMES = 30`: the first lit frame *arms* the full dump and it
fires thirty frames later, because the instant the shader finishes compiling
is also the instant the UART writer queue floods (the PR #300 interleaving
defect). On a clock-free project those are the same bytes — which is exactly
why the fixture is clock-free — and the test's own *"every later frame is the
same frame"* assertion is what makes comparing (a) with (b) legitimate rather
than assumed.

**The first *lit* frame, not the first frame.** The frames before it are the
compile-window black fallback (ADR
`2026-08-03-memory-pressure-at-compile-safe-points`). Comparing an open-time
black frame to a lit oracle is the walk's own documented trap; `first_lit()`
is the first decoded frame with any non-zero byte on the wire.

**⚠️ The order rule.** If the decoded frame comes out as a **per-pixel byte
swap** of the oracle, the *order assumption* is wrong — and **the decoder is
not to be "fixed" to match**. What to do with any other difference:

| the frame differs from | what it is |
|---|---|
| `[ORACLE]` alone (but equals `[ORACLE-RV32]`) | a **compiler** finding — native codegen vs wasmtime, `docs/defects/2026-07-30-q32-native-vs-wasmtime-last-bit.md`. Start at `lpvm-native`. |
| **both** host engines | a **machine** finding — the JIT's install path, the cache, a peripheral. The two engines agree with each other on this project. |
| reading (a) only (the pad disagrees with the firmware's own dump) | the RMT encode, the colour order, or the refill path — **the comparison a board cannot make, and the reason this walk exists**. |

**⚠️ The channel is not fixed at open; the pad is.** The classic's driver binds
a **wire** index, and its open line says so:

```text
Esp32V3RmtWs281xDriver::open: endpoint=… gpio=/gpio/18 wire=0 bytes=192 (slot per transmission)
[OUT] open endpoint=… bytes=192 leds=64 (frame-dump build)
```

`wire_pusher.rs` chooses an RMT slot per transmission and routes the pad to it
with a `func_out_sel_cfg` write, so the channel a frame goes out on can change
between frames. Everything here is keyed on the **pad**, which is why that is
fine — but a reader who expects a fixed channel will misread a trace.

#### Five wires over four slots

`plan_for_declared` caps the plan at `POOLED_SLOT_CAP = 4` two-block slots
(`v3_rmt.rs`), so a fifth wire **time-shares** one by per-transmission pad
muxing. That second wave only runs on the product path — the pusher is on core
1, driven by mailbox posts from the PRO core's outputs — so exercising it
needs a project that really declares five outputs: `projects/test/five-wire`,
on the board's own IO18 / IO16 / IO14 / IO2 / IO13 (`../bench.md`: L0 read
those same five wires off the running board).

The re-mux is visible in the **pin log**, not in `routed_pads()` — that call
answers where a pad is routed *now*, and the second wave is a statement about
routing over time:

```text
# route gpio18 <- RMT_SIG_0 (out_sel=87 inv=0)     IO18 and IO13 time-share
# route gpio16 <- RMT_SIG_2 (out_sel=89 inv=0)     channel 0, wave by wave
# route gpio14 <- RMT_SIG_4 (out_sel=91 inv=0)
# route gpio2  <- RMT_SIG_6 (out_sel=93 inv=0)
# route gpio18 <- GPIO_OUT                         parked between waves
# route gpio13 <- RMT_SIG_0 (out_sel=87 inv=0)
```

⚠️ The pusher deliberately starts a queued second-wave frame **a wave late**
(`shared_driver.rs`). That is by design, and nothing in `tests/five_wires.rs`
gates on frame timing — only on bytes and counts. There is one more reason not
to: the pusher lives on the other core, so a different `--core-quantum` starts
the same frame **64 cycles apart**, one window. Measured over 2,000 frames a
pad, every frame the two runs shared was byte-identical on every pad — and the
re-muxed pad, gpio13, carried 2,008 frames at quantum 256 and 2,009 at 64,
because the deadline is a *cycle* and one window decides whether the last wave
starts before it. `tests/pin_frames.rs` can compare two quanta's dumps
including their times; it drives its waveform from the host on one core.
`tests/five_wires.rs` compares the frames' *shape* up to the last one both
runs finished, and allows the counts to differ by one — that difference is the
second wave's signature rather than a flaw in either.

#### All three readings, measured

`just walk-esp32v3-emu-frame` — the live `lp-cli upload`, not a replay —
loads the project, opens the output, compiles the shader, and runs on to its
own emulated deadline (10 s of guest time) with nothing unmapped:

```text
===== COMPARISON =====
  pad 18: 2438 frame(s) decoded, 2437 lit, 1 distinct lit frame(s)
PASS: pad 18 == [ORACLE] rgb (384 hex chars), 2437 lit frame(s),
      all of them the same bytes.
PASS: the frame is byte-identical on all THREE readings (384 hex chars).
  [OUT] dump == pad 18 == [ORACLE] rgb

run: cycles=2400000000 instructions=1096173276 (core0=840485273 core1=255688003)
     idle=1595005 unmapped=0 (reads 0, writes 0, 0 sites) fence=0 quantum=256
pin gpio18: 2438 frames, 2438 complete, 0 errors, 64 leds, 7489536 edges
```

`tests/shader_oracle_pin.rs` says the same thing per tick — `crc=0x55772254`,
the deferred `[OUT] dump frame=31`, two runs' `--dump-frames` sha256-equal —
and `tests/five_wires.rs` adds the five wires: five distinct byte strings,
each equal to a `[OUT] frame=… crc=` line the guest itself printed, ~2,009
whole frames a pad, zero bit errors.

Until **M4 P4b** (PR #711) none of that was reachable: loading any project
killed the guest in a ROM window handler a few seconds in, so the deferred
dump — thirty frames past the first lit one — never printed. That is "[The
window, across a context save](#the-window-across-a-context-save)" below, and
it was the machine, not the firmware. The one thing it still decides is
nothing: `walks/shader-oracle.script`'s 30 ms chunk gap used to choose *where*
the guest died, and is now only how fast the upload lands.

#### ⚠️ The console interleaves, so the gates read a repaired one

The classic's writers yield mid-line, so a record that has put half of itself
into the TX FIFO can have another task's whole record land inside it — the
same PR #300 defect that makes `frame_dump` defer its lit dump. It is not
rare: the driver's own open line arrives cut in four.

```text
…: Esp32V3RmtWs281xDriver::open: endpoint=esp32v3-rmt-ws281x[stack] heartbeat: …
[MEM] free=177468 used=64084 …
[JIT] used=0 peak=0 cap=65536 …
:ws281x:local:IO18 gpio=/gpio/18 wire=0 bytes=192 (slot per transmission)
```

Both test files carry a `deinterleave` that rejoins a cut record around its
insertions — nothing dropped, nothing loosened — and every console assertion
reads the repaired console. It matters most for reading (a): a split would
truncate its 384 hex characters into a near-miss that looks like a *wrong
frame* rather than a missing line.

#### ⚠️ The guest's frame counter is only visible every sixtieth frame

`frame_dump::report` prints one `[OUT] frame=<n> … crc=…` line per
`REPORT_EVERY_FRAMES = 60` frames, so the highest `n` on the console is the
last multiple of sixty the guest reached and **not** its total — 1,980
reported against 2,009 carried, on a run that dropped nothing. "The counts
agree within one" is a claim about where the deadline fell. The claim that is
about dropped frames, and the one `tests/five_wires.rs` makes, is that the pad
carried at least every frame the guest counted and under one report period
more.

### The walk, whole — the boot chain and the cable

*M5 P5. `just walk-esp32v3-emu`, and `just walk-esp32v3-emu-frame` for the
half above.*

The frame walk asks the right question from a direct load with no cable in
it. A hardware walk flashes a board, resets it over a CH340, opens a tty,
uploads, and lets go of the port; the twin of that is one script with two
front doors, and the short name is the bigger thing:

| | `walk-esp32v3-emu` | `walk-esp32v3-emu-frame` |
|---|---|---|
| boot | ROM-up from a merged 4 MiB image | direct load |
| cable | `--control tcp:` + `--reboot-on-reset` | none |
| cost | ~2 min 45 s wall | ~2 min |
| asks | the same question, from further back | the frame, three ways |

The cable half is the part a hardware walk gets from a desk and never writes
down, so it is written down here. One control client for the whole run:

```text
===== CABLE =====
attach           ok attach cyc=2640000 us=11000
reset            ok reset cyc=2880000 us=12000
open             ok open cyc=240000 us=1000
state            ok state cyc=480000 us=2000 cable=attached port=open dtr=0 rts=0 en=1 io0=1 strap=app reboots=1
…
===== CABLE (released) =====
close            ok close cyc=334800000 us=1395000
detach           ok detach cyc=335040000 us=1396000
state            ok state cyc=335280000 us=1397000 cable=absent port=closed dtr=0 rts=0 en=1 io0=1 strap=app reboots=1
```

Read the cycles: `reset` lands at 2,880,000 and `open` at 240,000, because
the reset's **release** rebooted the chip and the guest clock went back to
zero ("[The verbs, and why the reboot is an edge](#the-verbs-and-why-the-reboot-is-an-edge)").
The console then holds two boots — the ROM banner the reset cut mid-line and
the whole boot after it — which is what a reset board's transcript looks
like, and is the walk's own evidence that the cable did something. The final
`state` is **asserted**, not admired: `port=closed`, `cable=absent`, both
lines slack, `reboots=1`.

⚠️ **What the reset does not buy is a second boot.** A reboot restores the
power-on snapshot and that snapshot includes the flash chip, so the boot after
the cable reset formats the same blank `lpfs` the first one did. On silicon
every capture is a second boot because espflash hard-resets after *writing*;
the emulated twin of that is a two-run recipe and is M5 P6's.

**There are no `--wait-for` / `--chunk-gap` flags, and there is nothing for
them to do.** UART0 has no RTS/CTS, so on silicon a host that writes faster
than the guest drains loses bytes — but `UartEngine::poll_source` delivers a
live socket's bytes *at the programmed baud*, one symbol apart, so at 921,600
baud at most ~92 bytes reach the 128-byte RX FIFO per millisecond and
`io_task` drains it on a 1 ms pacer. The host cannot outrun the wire here even
when it tries. The 30 ms chunk gap in the committed `walks/*.script` replays
is a **run parameter** and always was.

### The heap gate, read from this machine

*M5 P5. `just heap-budget-check-chips-v3`, `scripts/heap-budget-check.sh`,
`scripts/heap-budget-record.json`.*

`scripts/heap-budget-record.json`'s `chips` section is what the *firmware*
costs, as opposed to what a project costs, and since M5 P5 the classic has a
row in it beside the C6's. The shipped image (`esp32,server,float-f32`) is
direct-loaded here and read from its first heartbeat triple:

```text
heap-budget: booting esp32v3 (esp32,server,float-f32) on lp-emu:esp32v3:t1
  ok: totalBytes: 241552          ok: usedBytes: 17044
  ok: freeBytes: 224508           ok: largestFreeBlock: 108526
  ok: stackTotal: 45280           ok: stackHighWater: 16060 B (band 15500..16600)
```

⚠️ **The triple is elicited, not idle-emitted**, and that is the one thing a
reader copying the C6's arm gets wrong. `esp32_memory_stats` runs on a project
load/unload/stop-all or a client `runtime_status`, never on the five-second
server heartbeat — a classic boot with nobody talking prints no `[MEM]` line
at all. The gate asks, with
`lp-emu/lp-emu-validate/walks/v3-stop-all.script`: the same bytes on the same
trigger as `tests/boot_idle.rs` and as the desk sitting, and it stops on the
first `[JIT] used=`, 119 ms into the boot.

The four allocator figures are exact or ratcheted; the `[stack]` high-water is
a **band**, because a differently laid-out image has a different deepest
point. The classic's band is **measured on one host** — 16,060 B, twice,
byte-identical — and the record says so in `stack_band_note` rather than
implying a spread nobody has read. A runner's figure is M5 P6's to add.

It runs in CI's `Emulator ESP32v3 (x64)` job and not the C6's heap job: the
ELF is an Xtensa cross-build only that job installs, and the heap job's path
filter (`emu_c6`) does not fire for `lp-fw/fw-esp32v3/**` at all.

### The sinks

`--dump-frames <-|stdout|file:PATH>` writes one `ws281x-frame` JSON line per
frame **as it is decoded** — a stream, not a report, so a run that is killed
still leaves the frames it had already seen. Frames are kept in memory either
way, up to `FRAMES_PER_PAD_CAP` (8,192) per pad with a warning past it; the
stream is not capped.

```json
{"kind":"ws281x-frame","pad":18,"signal":"RMT_SIG_0","n":0,
 "start_us":0.000,"end_us":57.600,"bits":192,"leds":8,
 "wire":"5a4f...","rgb":"4f5a...","errors":0,"trailing_bits":0,
 "reset_us":357.600,"complete":true}
```

`wire` is what the wire carried (GRB for a WS2812) and `rgb` is that
unpermuted with `--strip-order`: the frame the *driver* was handed. Both are
in the record on purpose — a wrong assumption about the strip's order is then
a visible difference between two fields rather than something silently baked
into one.

`--pin-log <path>` writes the raw edge stream, one line per edge, with a
`# route` note whenever the matrix moves a pad:

```text
# route gpio18 <- RMT_SIG_0 (out_sel=87 inv=0)
0.000 gpio18 1 cyc=0
0.400 gpio18 0 cyc=96
1.250 gpio18 1 cyc=300
```

The **cycle** is the number anything may compute with; the microseconds are
for a human, and nothing is ever gated on an emulated microsecond (PD9). It
is off by default and capped at `PIN_LOG_LINE_CAP` (2,000,000 lines, with a
closing note): a 300-LED frame is 14,402 edges and the desk board's five
wires at 30 fps are over two million a second.

`--strip-timing ws2812|ws2811` and `--strip-order rgb|rbg|grb|gbr|brg|bgr`
say how every routed pad is read; the defaults are the driver's own, WS2812
and GRB.

At exit the CLI flushes any frame still in flight — reported **incomplete**,
with no `reset_us`, rather than invented — and prints one line per routed pad:

```text
pin gpio18: 22 frames, 22 complete, 0 errors, 256 leds, 12288 edges
```

### The name table

`src/regs/output_signals.rs` is **hand-written**, unlike its neighbours in
`regs/`: the GPIO matrix's signal enumeration is not a register block and is
not in the PAC at all, so `pac-regnames.py` has nothing to read. It carries
only the signals a trace has to *name*; anything else prints as `sig<N>`,
which is honest about the table being partial.

> ⚠️ **The classic's two enumerations do not agree, and mixing them mislabels
> a trace line without failing anything.** `OutputSignal::RMT_SIG_0` is **87**;
> `InputSignal::RMT_SIG_0` is **83**, so **87 is `RMT_SIG_4` on the input
> side**. The two tables are separate and neither lookup falls back to the
> other. `OUT_SEL_GPIO` is **256** here, where the C6's is 128 — and 128 is a
> real peripheral signal on this chip.

### The decoders ride the snapshot

A decoder caught **mid-frame** carries real state: the partial byte, the bit
count, the cycle the current pulse started. `Snapshot::pins` carries it, along
with the frames each pad has completed and what each pad is routed to. The
**sinks do not** — a file handle is not state — and the routing epoch is reset
on restore so the first drain afterwards re-reads the fabric rather than
trusting a number from another run.

### ⚠️ What the shipped image does **not** do on its own

The shipped `fw-esp32v3` configures its four two-block slots at boot and then
**starts no channel until a project's output opens**, which needs an `lp-cli
upload` over UART0. That was blocked on ruling **R6** when M4 P3 wrote
`tests/pin_frames.rs`, which is why that test drives the shipped image's own
register sequence through the machine from the host side rather than waiting
for the guest to issue it — every register value in it was read out of a
`--trace-block RMT` run of the shipped image. R6 is **fixed** (M4 P3b, "The
link, after the boot settles" below) and the walk scripts in `walks/` now do
the upload for real, so "A frame three ways" above is the guest-driven
reading; `pin_frames.rs` stays the host-driven one, and the two answering the
same way is worth more than either alone.

## The link, after the boot settles

*M4 P3's investigation of ruling **R6**, and M4 P3b's answer. **Fixed** —
the cause was the hart's poll point (c), not the timer pair; the P3
write-up stands as the record of where the dig started, and "Fixed" below
is where it ended.*

**The symptom.** A `--uart0-script` request fired `after "[RECOVERY] boot
complete (first frame served)"` is never answered, and neither is a
`then +Nms` follow-on after a request that *was* answered. The boot-idle gate
(`tests/boot_idle.rs`) only works because its script fires on `[INIT] I/O task
spawned`, during the busy boot.

**It is not the UART, the script, or the link.** A `--trace-block UART0` run
of the two-request script shows both requests arriving in full and being read
out of the RX FIFO by the guest:

```text
cyc=30053426 poll_rx_into+0x52  R4 UART0+0x01c status = 0x00000015   21 bytes waiting
cyc=30053490 poll_rx_into+0x113 R4 UART0+0x000 fifo = 0x0000004d      'M'
…
cyc=30293675 poll_rx_into+0x113 R4 UART0+0x000 fifo = 0x0000000a      '\n'   request 1, whole
cyc=42293906 poll_rx_into+0x113 R4 UART0+0x000 fifo = 0x0000000a      '\n'   request 2, whole
```

So `ScriptedSource`'s `after`/`then` steps resolve, `UartEngine::poll_source`
re-arms, and `io_task` drains the FIFO on its 1 ms pacer for the whole run.
**The RX path is healthy**, and the three suspects the phase file named —
the byte source's re-arm, UART0's RX line, and swi2 — are all refuted by this
one trace.

**What is actually dead.** The guest's **thread-mode embassy executor** — and
with it `run_server_loop`, the only thing that reads the queue `io_task` is
filling. On the same run, symbolised against the image:

```text
cyc=27652505 <TimeDriver>::arm_next_wakeup+0x21b  W4 TIMG0+0x000 t0.config = 0xc0002c00
cyc=27892410 InterruptStatus::current             R4 DPORT+0x0ec core_0_intr_status0 = 0x0000c000
cyc=27892860 SchedulerState::resume_task+0xa9     W4 DPORT+0x0dc cpu_intr_from_cpu0 = 1
cyc=27892918 timer_tick_handler+0x285             W4 TIMG0+0x000 t0.config = 0xc0002800
cyc=27895203 Executor::run_inner+0x1a7            W4 DPORT+0x0dc cpu_intr_from_cpu0 = 1
   …and from here to the end of the run, only:
cyc=…412581  io_pacer_isr                         W4 TIMG0+0x0a4 int_clr = 0x00000002
cyc=…412671  __pender+0x2d                        W4 DPORT+0x0e4 cpu_intr_from_cpu2 = 1
cyc=…413426  poll_rx_into+0x52                    R4 UART0+0x01c status = 0x00000000
```

The last TIMG0 **t0** alarm is armed at 27,652,505 and fires at 27,892,410.
`timer_tick_handler` wakes the executor thread (swi0), clears the alarm, and
then `arm_next_wakeup` writes **nothing** — which in esp-rtos 0.3.0 means the
embassy timer queue's next wakeup is `u64::MAX`. The executor polls once more
and parks in `ThreadFlag::wait` at 27,895,203, which sleeps the task with
`Instant::EPOCH + Duration::MAX` — **no timer at all**. The server loop's own
`embassy_time::Timer::after(1 ms)` at the bottom of `run_server_loop` was
never re-registered, and `StreamingMessageRouterTransport::receive` is
`try_receive` with no waker, so `io_task`'s `try_send` of the parsed `M!` line
wakes nobody. The board is then awake forever on the 1 ms pacer and asleep
forever everywhere else.

**Why it is the classic's and not the firmware's.** The C6 runs the same
`fw_esp32_common::server_loop` with the same 1 ms yield, and its committed
walks open with **exactly this needle** — `walks/examples-basic.script`'s
first line is `after "[RECOVERY] boot complete (first frame served)" …`,
followed by dozens of requests that are all answered. The classic-only parts
of this path are the esp-rtos time-driver **pair** (TIMG0 `t0` for the alarm,
TIMG0 **LACT** for `esp_rtos::now()`), the swi2 interrupt executor, and the
TIMG0 `t1` pacer. That pair is where the next dig starts, and the margin is
worth knowing: on the last successful tick, `now` (LACT) was **3 µs** past the
deadline the alarm had been armed for, out of 1,000.

**Reproducer, for whoever picks it up** (nothing below is a workaround — the
phase file's rules stand: no concatenated script steps, no widened
`--exit-on`):

```bash
printf '%s\n%s\n' \
  'after "[RECOVERY] boot complete (first frame served)" +1ms "M!{\"id\":1,\"msg\":\"stopAllProjects\"}\n"' \
  'then +50ms "M!{\"id\":2,\"msg\":\"stopAllProjects\"}\n"' > /tmp/two.script
cargo run -p lp-emu-esp32v3 --release -- --elf <fw-esp32v3> \
    --strict-bus --core-quantum 256 --timeout 20s \
    --uart0-script /tmp/two.script --uart0 file:/tmp/run.log \
    --trace /tmp/run.trace --trace-block DPORT --trace-block TIMG0
grep from_cpu0 /tmp/run.trace | tail -2   # the last swi0 is ~27.9 M cycles
```

It reproduces at `--core-quantum` 64, 256 and 4096, at the same point in the
boot each time, so it is not a fine-grained interleaving race.

**Fixed (M4 P3b): the hart dropped its own software interrupt.** Not the
timer pair. The two counters are one clock in this model as on silicon, and
the "3 µs" is ISR latency, not drift: `Timer::after(1 ms)` read LACT at
cycle 27,651,924 (107,844 µs, so a deadline of 108,844), `arm_next_wakeup`
read it again at 27,652,122 (`lactlo = 0x001a5452`, 107,845 µs) and armed
`t0` for 999 µs (`t0.alarmlo = 0x00009c18`, 39,960 ticks at 40 MHz), the
alarm fired, and the tick handler read LACT at 27,892,580
(`lactlo = 0x001a92f1`, 108,847 µs) — three microseconds past a deadline it
had set two reads earlier, on the same clock, with a `u64` division and an
interrupt entry in between. The handler did everything right: it processed
the embassy queue, **woke the server task**, and computed `MAX` because the
queue was then genuinely empty. What was lost came after.

The one register the P3 trace could not show is the executor thread's saved
context. Two more traces did — the bus trace of the same run, and an
instruction trace stepped from cycle 27,894,500 of it — and between them they
discriminate the three hypotheses P3b was handed:

```text
cyc=27652505 <TimeDriver>::arm_next_wakeup+0x21b    W4 TIMG0+0x000 t0.config = 0xc0002c00   the one and only arm of the run
cyc=27652595 Executor::run_inner+0x1a7              W4 DPORT+0x0dc cpu_intr_from_cpu0 = 1   flags.wait(): the task is Sleeping, swi0 raised
cyc=27652669 Executor::run_inner+0x1a7              W4 DPORT+0x0dc cpu_intr_from_cpu0 = 1   …74 cycles later, again
   …seven raises to cyc=27653039: the level-1 interrupt is never taken, the loop keeps lapping
cyc=27653203 InterruptStatus::current               R4 DPORT+0x0ec core_0_intr_status0 = 0x01008000   t1 (bit 15) lands — AND swi0 (bit 24), still there
cyc=27653546 cross_core_yield_handler+0x1d          W4 DPORT+0x0dc cpu_intr_from_cpu0 = 0   the switch, 950 cycles after the first raise
cyc=27892410 InterruptStatus::current               R4 DPORT+0x0ec core_0_intr_status0 = 0x0000c000   the t0 alarm
cyc=27892706 embassy_executor::raw::waker::wake                                            the server task goes on the executor's run queue
cyc=27892860 SchedulerState::resume_task+0xa9       W4 DPORT+0x0dc cpu_intr_from_cpu0 = 1   …and its __pender resumes the executor thread
cyc=27894564 cross_core_yield_handler+0x1d          W4 DPORT+0x0dc cpu_intr_from_cpu0 = 0   the switch back
cyc=27894882 esp_rtos::now+0xb                      W4 TIMG0+0x080 lactupdate = 1          run_scheduler's clock read — the only read in the wake
cyc=27895148 pc=0x400d2cbf run_inner+0x1d7          movi.n a8, 1                            resumed: the instruction AFTER take_all's s32c1i
cyc=27895152 pc=0x400d2cc9                          beqz a10, +0x219                        a10 is the CAS result from cycle ~27,653,000
cyc=27895153 pc=0x400d2d01                                                                  "the queue is empty"
cyc=27895184 TaskExt::set_state                                                             Sleeping, for ever
```

Hypothesis 1 (parked on some other future): **no** — the executor polled
nothing. Its last poll before the park is the `take_all` above, whose result
was stale; the task is on the run queue and stays there. Hypothesis 2 (`t0`
misread at the fire): **no** — between the fire at 27,892,410 and the
non-write there are three `t0.config` accesses (esp-hal's `clear_interrupt`)
and no `t0.lo`/`t0.hi` read at all. The director's hypothesis (LACT ahead of
`t0`): **no** — the numbers above.

**What actually happened.** `ThreadFlag::wait` marks the executor thread
Sleeping, raises swi0 through a DPORT store *inside* a critical section, and
expects the interrupt at the `wsr PS` that closes it. The hart's poll point
(c) — the re-sample after any MMIO store — read
`Bus::pending_cpu_interrupt()`, the RV32 matrix's single-line answer, widened
to a mask. The Xtensa matrix answers that `None`, because `INTENABLE` and
`PS.INTLEVEL` are the hart's own registers; widened, that is **0**. So every
MMIO store on this machine zeroed the asserted-line mask the machine had fed
at the slice boundary — and the store that raised swi0 hid swi0. The hart
saw it again only at a later boundary that happened to fall outside the
critical section, or, as here, when an unrelated line (the 1 ms pacer) made
it re-sample. In the 950 cycles between, a thread the OS believed asleep
lapped its loop seven times and was finally switched out mid-way through
`dequeue_all`'s atomic swap, with the swap's result in `a10`. The tick handler
then pushed the woken task onto that same queue; the resumed thread compared
its stale `a10` with zero, took the empty branch, and slept at
`Instant::EPOCH + Duration::MAX`. `push_was_empty` answers `false` for a
non-empty queue, so no later pend could ever fire again. The `lp-xt-emu`
module docs had carried this as "the approximation at poll point (c) is
retired when M3's Xtensa machine binds a hart to a `SocBus`" — M3 fed the
honest mask between slices and never retired the approximation inside one.

The fix is the hart reading the mask form: `Bus::pending_cpu_interrupt_mask`
(new, defaulting to the single-line answer widened, so every bus that only
implements the RV32 form keeps its behaviour), overridden on `SocBus` with
the same `CpuIntMatrix::asserted` the between-slice feed uses, read by
`XtHart::resample_external`. `src/periph/timg.rs`, `engine::timg` and
`machine.rs` are untouched; the RV32 hart and the C6 machine keep reading the
single-line form. Pinned three ways: the hart's
`poll_point_c_reads_the_mask_form_and_keeps_the_lines_the_machine_fed`, and on
the shipped image `tests/uart_socket.rs`'s
`a_script_with_three_requests_is_answered_three_times` (ids 1, 2, 3 on the
wire, in order) and
`the_thread_executor_keeps_re_arming_its_tick_after_the_boot_settles` — which
counted **one** `t0` arm in two idle seconds before the fix and about two
thousand after. With the fix the two-request reproducer above answers both,
and the heartbeat reports `fps ≈ 979`: the server loop is running its 1 ms
tick for the first time on this machine.

⚠️ **Three things about this chip's tick worth carrying into M5's replay and
M6's S3.** (1) esp-rtos arms `t0` by *resetting it to zero* on every arm
(esp-hal's `OneShotTimer::schedule` is stop, clear, reset, load, start), so
`t0.lo` never counts past ~40,000 and is no use as a timestamp — LACT is the
only free-running counter, and `Instant::now()` costs seven reads of it
(esp-hal polls `lactlo` for the latch to "change", which in this model it
never does). (2) A settled classic wakes every millisecond from now on — the
idle skip cannot skip further than the next `t0` alarm, and a run's
instruction count is ~5× what it was when the executor slept. (3) The S3 has
the same shape — software interrupts raised by MMIO store, enables in the
CPU — so its bus must answer the mask form and must **not** implement
`CpuIntMatrix::cpu_interrupt`; `SocBus` already does the first.

## The window, across a context save

*M4 P4b.* P4 found, walking a project upload, that **loading any project
killed the guest a few emulated seconds in** — deterministically, at the same
cycle under `--core-quantum` 64 / 256 / 4096, on both boot paths, with and
without an output node:

```
STRICT BUS STOP
  pc      = 0x400800c9 (~_WindowUnderflow8+0x9)
  access  = Read Word at 0xfffffff4          (= a1 - 12 with a1 = 0)
```

The same image runs for hours on the desk board, so it was the machine. It
was the **hart's shared executor**, and one line of it: `CALL0`/`CALLX0`
wrote `PS.CALLINC = 0`.

### Reproducing it, and where the cycle comes from

`walks/shader-oracle.script` replays the whole upload over UART0 in guest
time. On the shipped image built from this tree (`63965f5c7`, the default
features) the stop is at cycle **1,093,494,221** (4.556 s); on P4's
`frame-dump` build of the same tree it is at 910,614,952 (3.794 s); P4
measured 617,813,647 on its own image. ⚠️ The cycle moves with the image
and the PC does not: the fault needs an interrupt to land on one particular
instruction, and which tick does so is a property of the instruction stream.
A run that stops short of the crash — the first attempt here ran 4 s and saw
34 clean frames — proves nothing; `tests/project_load_survives.rs` runs to 8 s
for that reason.

### The trace

Taken instruction by instruction on the shipped image (a local hook that
swapped the machine's `run_slice` for `run_slice_traced` from a chosen cycle;
`--trace` is the bus trace and does not do this — see "What this needed"
below). The level-1 pacer (`EXCCAUSE 4`) lands on core 0 with four live
CALL8 frames at bases 1, 3, 5 and 7:

```
@slice now=1093492288 pc=0x40080340 (_UserExceptionVector)
       wb=7 ws=0x00aa ps=0x00060d30 epc1=0x400880b0 a1=0x3ffddc40
```

`ps = 0x00060d30`: `CALLINC = 2`. `EPC1 = 0x400880b0` is an `entry` — the
interrupt landed **between frame 7's `call8` and its callee's `entry`**. Nine
instructions later the handler saves that PS into its exception frame:

```
I 0x40080340 wsr.excsave1 a0
I 0x40080343 rsr.exccause a0
I 0x40080346 beqi a0, 5, _AllocAException
I 0x40080349 call0 __naked_user_exception      <-- the one instruction between the two that touches CALLINC
I 0x40080a88 or a0, a1, a1
I 0x40080a8b addmi a1, a1, -256
I 0x40080a8e s32i a0, a1, 12                   W [0x3ffddb4c] <- 0x3ffddc40   XT_STK_A1
I 0x40080a91 s32e a0, a1, -12
I 0x40080a94 rsr.ps a0
I 0x40080a97 s32i a0, a1, 4                    W [0x3ffddb44] <- 0x00040d30   XT_STK_PS: CALLINC = 0
```

Everything the director's hypotheses named was then checked and was right.
`save_context`'s `SPILL_REGISTERS` (`and a12,a12,a12; rotw 3` ×4, `rotw 4`)
rotated 7 → 10 → 13 → 0 and raised exactly the three overflows the RM puts
there, each `_WindowOverflow8` because each victim's callee was a `call8`:

```
X rotw base 7 -> 10 ws=0x00aa ; -> 13 ; -> 0
I 0x40080080 _WindowOverflow8  (base 1)  a0..a3 -> [0x3ffddd50..)  a4..a7 -> [0x3ffdde60..)  rfwo ws=0x00a8
I 0x40080080 _WindowOverflow8  (base 3)  a0..a3 -> [0x3ffddcd0..)  a4..a7 -> [0x3ffddde0..)  rfwo ws=0x00a0
X rotw base 0 -> 3
I 0x40080080 _WindowOverflow8  (base 5)  a0..a3 -> [0x3ffddc30..0x3ffddc40)  a4..a7 -> [0x3ffddd40..)  rfwo ws=0x0080
X rotw base 3 -> 7
```

Frame 5's `a0..a3` went to `[0x3ffddc30..)` — sixteen bytes under frame 7's
`a1 = 0x3ffddc40`, the right place. A level-2 interrupt then nested inside
the handler (`ws 0x0580 → 0x0400`, one `_WindowOverflow4` and one
`_WindowOverflow8`), returned through `rfi 2`, and the level-1 handler's own
returns reloaded through `_WindowUnderflow8` (base 10 → 8) and
`_WindowUnderflow4` (8 → 7) with every `PS.OWB` and every address right.
`restore_context`'s `l32i a1, a1, 12` read `0x3ffddc40` back. Then `rfe`,
and the first instruction after it:

```
I 0x400880b0 entry a1, 128        X entry base 7 -> 7 ws=0x0080
                                  R a1=AR[29] <- 0x3ffddbc0
```

The re-run `entry` rotated by **zero** — `CALLINC` came back from the
frame as 0 — and wrote the callee's stack pointer into **frame 7's own
`a1`**, 0x80 below where it was. The callee then ran in frame 7's window,
and 141 cycles later frame 7's `retw` (`a0 = 0x8008884f`, a `call8` return)
took `_WindowUnderflow8` at base 5 with `a9 = 0x3ffddbc0` instead of
`0x3ffddc40`:

```
I 0x40088279 retw
I 0x400800c0 l32e a0, a9, -16     [0x3ffddbb0] = 0        (the exception frame's ACCHI slot)
I 0x400800c3 l32e a1, a9, -12     [0x3ffddbb4] = 0        (M0)
I 0x400800c9 l32e a7, a1, -12     0xfffffff4 -> STRICT BUS STOP
```

P4's "written only by `save_context`" reading of its save area was exactly
this: the `0x3f700000, 0` it saw were the `F14`/`F15` slots of its own
exception frame (its callee's `entry` was 32 bytes, so `a1` had shifted by
0x20 into `sp_exc + 0xd0`).

### The cause, from the RM

The ISA Reference Manual's CALL0 page (p. 297) gives the instruction exactly
two effects:

```
AR[0] ← PC + 3
nextPC ← (PC31..2 + (offset17 12||offset) + 1)||00
```

CALL4's (p. 299) is `WindowCheck (00, 00, 01)`, **`PS.CALLINC ← 01`**,
`AR[0100] ← 01||(PC + 3)29..0`, and Table 4–113 says of CALL4/8/12 that
"These instructions communicate the number of registers to hide using
PS.CALLINC in addition to the operation of CALL0." `PS.CALLINC` is
architectural state that lives for one instruction per windowed call, and
xtensa-lx-rt's vector does a `call0` inside that instruction — so does
ESP-IDF's. Silicon never noticed because silicon's CALL0 does not write it.
`lp-emu/lp-xt-emu/src/executor/call.rs` did, and the hart, `save_context`,
the spills, the underflows and `rfe` all faithfully carried the zero.

None of the five hypotheses in the brief was it — `rotw`'s mapping, the
`s32e`/`l32e` addresses, the per-instruction check while rotated,
`wsr.windowstart`, `PS.OWB` across the nest — and the trace above is the
evidence for each. The fault was upstream of all of them.

### Why nothing caught it

- **The mach fixtures raise their interrupts with `wsr.intset`**, which the
  hart delivers on the very next instruction (poll point (b)) — a `call8`
  never sits there — and their scripted external line lands at slice
  boundaries the fixture does not choose. `mach_ctxswitch`'s twelve scripted
  preemptions never fell in a gap.
- **The user-mode runner has no interrupts**, so its ENTRY always sees the
  CALLINC its own call just wrote; `WindowPolicy::Direct` also takes
  `max(CALLINC, 1)`. The corpus goldens and the fp silicon replays are
  byte-identical under the fix.
- **The C6 boots to its heartbeat and `shader-compile-stress` compiles with
  `unmapped = 0`** because neither loads a project: a render-loop tick is
  what puts a 1 ms interrupt inside a `call8`/`entry` gap often enough.

### What pins it

- `lp-xt-emu`'s `mach_callinc` fixture (`lp-xt/fixtures/mach/src/bin/`): a
  CALL8 chain, a `CCOMPARE0` match placed on a callee's `entry` by scanning
  `k`, lx-rt's real vectors, and the returns. Its transcript with the fix:

  ```
  vectors entered: 0x340 x8, 0x080 x14, 0x000 x1, 0x0c0 x13
  gap rounds (EPC1 == p4b_leaf): 1
  first gap round k: 6
  PS.CALLINC the handler saw on it: 2
  leaf result on it: 101
  windowstart in d3: 0x02ad
  ```

  On `origin/main` the same run derails on the gap round and dies in
  1,599,729 double exceptions.
- The hart's `a_call0_inside_the_vector_leaves_ps_callinc_for_the_interrupted_entry`
  unit test: red on main with `saved PS = 0x00040030`.
- `tests/project_load_survives.rs`: the shipped image, this script,
  `--strict-bus`, 8 s; `Outcome::Deadline`, `unmapped = 0`, the load
  answered, the heartbeat after it, and ≥ 60 whole frames on pad 18.

### What this needed, and does not have

There is no committed way to trace the hart's instructions at machine level:
`--trace` is the bus trace, `--break-at` stops once. The trace above came
from a 60-line local patch that swapped `run_slice` for `run_slice_traced`
past a cycle and printed the hart's window state at every slice boundary;
P4's report says it stepped "instruction by instruction with a memory
watch" the same way. Two phases have now built that tool and thrown it away.
A `--trace-inst <cycle>` door on this binary is the obvious next spend, and
it belongs to whoever next needs it, not to this fix.

## Flash, and the cache window

*P4 lands the tables and the enable bits; P7 lands the chip and the fill.*

The flash MMU is **two raw 2048-entry `u32` arrays** at `0x3FF1_0000` (PRO)
and `0x3FF1_2000` (APP) — inside the DPORT window but past the end of the
register block svd2rust generates, so nothing in the PAC names them. Every
number in `src/cache.rs` comes from the mask ROM's own disassembly, quoted in
the module header: `mmu_init` (`0x4000_95A4`) for the tables' size and base,
`cache_flash_mmu_set` (`0x4000_95E0`) for the entry format and the four
windows' index bases, and `*_cache_ctrl1` bits 10:9 for the page size (the
PAC's `pro_cmmu_flash_page_mode` says the same from the other side).

⚠️ **The classic's entry is a bare physical page number with no valid bit** —
the opposite of the C6, whose `Cache_MSPI_MMU_Set` ORs bit 9 in. The ROM sets
none and tests none, and `mmu_init` clears the table to zero, so a zeroed
entry means *flash page 0* rather than "unmapped". `translate` therefore
answers `None` only for an address outside the four windows, and P7 — the
first code that can tell a mapped page from an unwritten one — decides what an
unwritten entry serves.

The four windows and their index bases, from the ROM:

| virtual window | entries |
|---|---:|
| `0x3F40_0000..0x3F80_0000` (DROM) | 0..63 |
| `0x400D_0000..0x4040_0000` (IROM) | 64..127 (`0x400D_0000` is entry 77) |
| `0x4040_0000..0x4080_0000` | 128..191 |
| `0x4080_0000..0x40C0_0000` | 192..255 |

### The chip, and where its bytes come from

`src/flash.rs` holds this board's numbers — 4 MiB (the desk board), the
bootloader at **`0x1000`** (⚠️ *not* the C6's `0x0`), the partition table at
`0x8000`, `factory` at `0x10000` and `lpfs` at `0x310000` — over the chip
model in `lp_emu_esp_common::engine::spi_flash`. Program is an `&=`, erase is
the only way back to `0xff`, and the JEDEC capacity byte and the ROM's
`chip_size` word are derived from the same length, so `esp_storage`'s
`flash_rdid` decode and the ROM's own bounds check describe one part.

Three backings, and `tests/flash_persistence.rs` is the proof:

| flag | what it does |
|---|---|
| *(none)* | a blank chip that lives and dies with the process |
| `--flash <path>` | read at start, written back at the end of the run; a path that does not exist is created blank |
| `--flash-copy <path>` / `--merged <path>` | read once, never written — a scratch copy of a known image |

### ⚠️ `SPI1.addr` is packed two ways, and the trigger says which

The generic `usr` engine shifts its address phase out of `addr` **MSB-first**
for `user1.usr_addr_bitlen` bits, so a 24-bit flash address sits
**left-justified in bits 31:8**. The dedicated `flash_pp` / `flash_se` /
`flash_read` triggers take the address from bits **23:0** with the byte count
in bits 31:24 — the mask ROM says so in one instruction
(`SPI_page_program` `0x4006_2368`: `slli a10, a5, 24` / `and a9, a3,
0xffffff` / `or` / `s32i` to `SPI1+0x04`).

The C6 has only the second convention. Carrying it across reads the right
register and asks for an address 256 times too big, which the first run of
this block did:

```text
W4 SPI1+0x02c miso_dlen = 0x000001ff
W4 SPI1+0x004 addr      = 0x00800000
SPI1 read 64 bytes at 0x00800000 leaves the 0x400000-byte chip
```

— the partition table at `0x8000`, refused as past the end of the part, 64
bytes of `0xff` handed back, and the firmware printing `[ERROR] no lpfs
partition in the flashed table`. A plausible failure a long way from its
cause, which is what this convention costs when it is got wrong.

### The fill

The window is served as a **cache fill**, not per-access translation:
`cache::fill` copies a whole page out of the chip into the RAM behind its
window whenever an MMU entry moves, the page mode changes, or the flash under
a mapped page is written. Instruction fetch stays a plain RAM read, which is
what keeps this machine fast enough to be used, and the price is stated
rather than hidden: **this model is stricter than silicon about staleness** —
a real cache serves stale lines until it is flushed, and this one never does.

Two consequences of the missing valid bit, both in `cache::fill`'s own words:

- a zeroed entry means flash page 0 and is **served**, because refusing would
  be inventing a bit the silicon does not have. `mmu_init`'s 8 KiB `memset`
  costs nothing anyway, because the MMU view only marks an entry dirty when
  the write *changes* it;
- entries **64..=76** name virtual addresses inside SRAM0, where the vectors
  and the IDF bootloader live. `FlashMmu::entry_vaddr` checks the round trip
  through `entry_index` and answers `None` for them, so a fill can never
  overwrite the code it is running.

And one this machine's shape forces: `SocBus` holds the two windows in a
single arena, so there is no per-core copy for the APP core's table to fill.
The fill serves the **PRO** core's table and says so out loud when core 1's
table disagrees — which is the case a per-core window would be needed for,
and M4's to answer.

The host-side fill never arms D4's check: it goes through
`SocBus::load_image`, and the machine marks the window around it with
`ClassicCache::set_host_access`.

### The cache-off fetch stop

*If a run just stopped with `CACHE-OFF FETCH`, this section is for you.*

```text
CACHE-OFF FETCH  core=0  cycle=1_284_991
  pc=0x400d1a2c (lp_flash::write+0x38)  fetch from IROM 0x400d1a2c
  this core's cache was disabled at cycle 1_284_610 by a write to
  DPORT+0x040 (pro_cache_ctrl.pro_cache_enable <- 0) from pc=0x40081c04
  (esp_storage::write+0x1c)

  On silicon this core stalls until the cache returns; nothing observes it
  except a crash or a watchdog. This emulator refuses instead.
  `--cache-off-fetch permit` continues (and claims nothing about the stall).
```

**What happened.** A core reached through a flash window — `0x400D_0000..`
for instructions, `0x3F40_0000..` for data — while its own read cache was
disabled. `pro_cache_ctrl.pro_cache_enable` (bit 3) is the bit; the ROM's
`Cache_Read_Enable` (`0x4000_9A84`) sets it and `Cache_Read_Disable`
(`0x4000_9AB8`) clears it, and firmware turns it off around a flash write
because the SPI controller cannot serve the cache and a program at the same
time. Code that runs during that window has to live in IRAM.

**Why it is a stop.** On a board the core stalls; here the window is ordinary
RAM, so the guest sails through. Refusing is the only way an emulator can
report a hang it cannot reproduce (plan **D4**), and it is on by default so
every gate run has it.

**How to turn it off.** `--cache-off-fetch permit`. That does not run the
check and swallow the answer — it does not check at all, so the run keeps its
full-speed slices.

**What it does not claim.** Not how long the stall would be. Not that silicon
would crash. Not that the access is a bug: a firmware that knows what it is
doing may be doing it deliberately. Only this: *this happened, here, while the
cache was off.*

**What it costs when nothing is wrong: nothing.** The check is installed only
while the running core's cache is off — a flash write's worth of instructions
— and a run whose guest never disables the cache never installs it.

## Peripherals

*P4, P5 and P6 between them gave eight blocks behaviour; the rest are accept
probes until P7 and P8.* One row per block, in registration order
(`machine::PERIPHERAL_REGISTRATION_ORDER`, which is the order the boot met
them). An **accept** block is a `RegFile` seeded from the PAC's reset values
(`src/periph/accept.rs`) with no behaviour; a **view** computes something a
register file cannot. Two registers in the whole machine deviate from the
PAC and each carries its reason beside it.

| block | base | aperture | grade | stop | what it is trusted for | owner |
|---|---|---|---|---|---|---|
| `DPORT` | `0x3FF0_0000` | `0x1000` | **view** | direct #1, cycle 29 | the two per-core interrupt maps and the status words → the matrix; the four software interrupts → `IrqLines` sources 24..27; `appcpu_ctrl_*` → the app-core control; `pro/app_cache_ctrl{,1}` → the cache. The clock/reset gates and `cpu_per_conf` stay written-and-read-back | P4 |
| `RTC_CNTL` | `0x3FF4_8000` | `0x140` | **view** | direct #2, cycle 107,539 | the asserted reset cause (**a deviation**), both halves of the CPU stall key, the RWDT's write-protect key, the clock and store registers | P5 |
| `APB_CTRL` | `0x3FF6_6000` | `0x80` | accept | direct #3, cycle 109,663 | `sysclk_conf.pre_div_cnt`, the tick confs, and `date` bit 31 — the chip revision's top bit (**a deviation**) | P5 |
| `TIMG0` | `0x3FF5_F000` | `0x100` | **view** | direct #4, cycle 109,989 | three counters (`t0`, `t1`, LACT), the RTC calibration, the MWDT gate | P5 |
| `I2C_ANA_MST` | `0x6000_E000` (AHB) | `0x20` | **view** | direct #5, cycle 143,014 | the analog world as a `{block, register}` store behind eight host ports; `busy` reads 0 | P5 |
| `TIMG1` | `0x3FF6_0000` | `0x100` | **view** | direct #6, cycle 3,563,841 | the same block at sources 18..21; the image only disables its MWDT | P5 |
| `GPIO` | `0x3FF4_4000` | `0x1000` | **view** | direct #7, cycle 3,564,113 | the routing matrix over the bus's signal fabric: **40 pads in two banks**, `func_out_sel_cfg` (9-bit `out_sel`, `256` = follow `GPIO_OUT`), `func_in_sel_cfg` (**256** input signals, constants 48/56), `out`/`enable` and their bank-1 twins — `enable` is what makes a routed pad *drive* — the `in_`/`in1` pair through IO_MUX's `fun_ie`, the sticky interrupt latch and both per-core outputs. `strap` is a **deviation**: the desk board's `0x13` | P8 |
| `UART0` | `0x3FF4_0000` | `0x80` | **view** | direct #8, cycle 3,564,269 | the FIFO pair on `engine::uart`, the shifter draining at the programmed baud in emulated time, `status.txfifo_cnt` counting down (the bit the ROM spins on), the thresholds, the receive timeout, the interrupt quad, `mem_rx_status`'s address pair (the RX-count errata), the auto-baud counters, and the host stream | P6 |
| `IO_MUX` | `0x3FF4_9000` | `0x94` | **view** | direct #9, cycle 3,568,671 | one field — each pad's `fun_ie` (bit 9) pushed into the fabric as its input enable — over a pad map that is **not in pad order** (`+0x004` is `gpio36`, `+0x044` is `gpio0`), transcribed from the generated table's own names and checked against them. Thirty-six pads: this part has no GPIO 28..31 | P8 |
| `SPI1` | `0x3FF4_2000` | `0x400` | **view** | direct #10, cycle 3,644,210 | the flash controller over `engine::spi_flash`: the `usr` engine and the dedicated triggers, the WIP/WEL latch the ROM spins on, the JEDEC id `esp_storage` decodes, and the two `addr` packings | P7 |
| `SPI0` | `0x3FF4_3000` | `0x400` | accept + refusal | direct #11, cycle 3,644,282 | the ROM's idle wait on `ext2.st`, `cache_fctrl` bit 0 (`Cache_Read_Enable`'s other half), and `spi_flash_attach`'s eight writes. A `cmd` trigger here **refuses** rather than inventing a JEDEC id | P7 |
| `EFUSE` | `0x3FF5_A000` | `0x200` | **view** | rom-up #1, cycle 7 | the fuse array, the read-data registers, and the read command as a **completion**; the MAC and chip revision | P5 |
| `UART1` | `0x3FF5_0000` | `0x80` | **view** | rom-up, cycle 30,992 | the same model at its own base and its own interrupt source (35). The ROM's `uartAttach` touches `+0x10` on every boot; the application never opens it, so it has no host stream and its bytes go nowhere | P6 |
| `FLASH_MMU` | `0x3FF1_0000` | `0x4000` | **view** | not reached by P3 | the two flash MMU page tables, written by the ROM's `mmu_init` and `cache_flash_mmu_set`; an entry write marks its page for P7's fill | P4 / P7 |
| `SHA` | `0x3FF0_3000` | `0xc0` | **view** | not reached by P3 | the `TEXT` window (message **and** digest read-back) and the four per-family strobe quads over `engine::sha`, including the **`load`** step the C6 has no equivalent for. SHA-1 and SHA-256 compute; SHA-384/512 refuse. The ESP-IDF bootloader's image hash is the only caller | P7 |
| `RMT` | `0x3FF5_6000` | `0x1000` | **view** | **both paths, last**: direct cycle 5,640,047 / rom-up 65,360,003 | eight TX engines over the **512-word RAM at `+0x800`**, the interleaved interrupt bits on source 47, and the symbol pump driving `RMT_SIG_0 + n` (output signal **87 + n**) into the fabric. Five quirks the C6's block does not share — see “RMT on the classic” below. Refill-lag telemetry, reported and never gated | M4 P2 |

The three deviations from the PAC, every one of them an input to the run
rather than a property of the part, and every one a builder parameter:

- `RTC_CNTL.reset_state` — POWERON_RESET in both cause fields, the value
  L0's banner printed (`rtc_cntl::DEVIATIONS`);
- `APB_CTRL.date` bit 31 — esp-hal's `eco_bit2`, the top bit of the major
  chip revision, which no eFuse word on this part carries
  (`accept::DEVIATIONS`);
- `GPIO.strap` — **`0x13`**, the strapping pins as the pads were latched at
  reset. The PAC carries no reset for it, and zero is not "no straps": it is
  the SDIO boot mode, which the mask ROM takes seriously enough to walk into
  `slc_init_attach`. The value is the register **printed**: the ROM's `main`
  loads it and passes it raw as the `boot:0x%x` argument of its banner, and
  the desk board's banner reads `boot:0x13 (SPI_FAST_FLASH_BOOT)`.
  `Esp32V3Builder::strap` overrides it. ⚠️ There is deliberately no
  download-mode default: the cable's `Strap::Download` says IO0 was low when
  EN was released, and what word `GPIO.strap` then reads is a second fact
  this repository has not measured (R8).

### eFuse block 0 is the desk board's, word for word

Not four synthesised words but the **seven `espefuse` read off the part**
(`loader::DESK_BLOCK0`), with the MAC and the chip revision overlaid on top
so `--efuse-mac` and `--efuse-rev` still move what they name and nothing
else. Three fuses on the boot path came with it that no derivation had
produced, and all three had been reading zero — which is a legal unburned
value, and therefore silent:

| field | value | why it matters |
|---|---|---|
| `CLK8M_FREQ` | `0x37` (55) | the mask ROM's XTAL detection multiplies the internal oscillator's calibration by it. At 0 the ROM concluded **26 MHz** for a board with a 40 MHz crystal |
| `MAC_CRC` | `0x7c` | left at 0 with a note saying the polynomial was out of reach. It did not have to be computed — it was read off the part |
| `CONSOLE_DEBUG_DISABLE` | 1 | the ROM's BASIC-console fallback is fused **off** on this part |

plus `CHIP_PACKAGE`, `CODING_SCHEME`, `ADC_VREF`, `CHIP_CPU_FREQ_RATED` and
every security fuse off, none of which this crate has to name.

Grades: every register is `documented` where the PAC calls it read-write and
the block pretends nothing, `modeled` otherwise (`RegFile::with_pac_grades`);
the analog master, which the PAC does not know, and every register of the
eFuse view are `modeled` throughout. Nothing is `measured`.

Not modelled, on purpose, and unmapped so a strict run says so: everything
neither boot has reached — `RTC_IO` (where the ROM-up path now stands, in
`gpio_pad_unhold` reading `dig_pad_hold` at `+0x74`), `SENS`, `RTC_I2C`,
`FRC_TIMER`, `FLASH_ENCRYPTION`, `SHA`, `RMT`, `RNG` and the WiFi window. Their **AHB mirrors** are unmapped for the same
reason and by the same rule (DD38): an alias needs a registered block to
point at, so a block gains its second address on the day it gains its first.

### Why TIMG0 is three things at once

The classic ESP32 has **no SYSTIMER**. Every other part on this roadmap has
a dedicated system counter that `Instant::now()` reads; this one does not, so
one timer group carries three unrelated jobs at the same time — and a fourth
that is not a timer at all:

| use | registers | evidence |
|---|---|---|
| the esp-rtos tick / alarm | `t0` (`+0x00..+0x20`) | `board/esp32v3/init.rs` hands `timg0.timer0` to `esp_rtos::start` |
| the 1 ms io pacer, Priority 1 | `t1` (`+0x24..+0x44`) | L0: `[INIT] I/O task spawned (… timg0t1 pacer 1ms)` |
| `Instant::now()` | **LACT** (`+0x70..+0x94`) | `esp-hal-1.1.1/src/time.rs:713-758`, the `#[cfg(esp32)]` `implem` |
| the RTC clock calibration | `rtccalicfg`/`rtccalicfg1` (`+0x68`) | `esp-hal-1.1.1/src/clock/mod.rs:276-473` |

So a single block answers the scheduler's tick, the I/O pacer's deadline,
every timestamp the firmware takes, and the measurement that decides what the
crystal is. It is why the shared `engine::timg` takes a **count** of counters
rather than assuming one, and it is the single most surprising fact about
this chip's timekeeping.

All three counters tick from APB through their own 16-bit prescaler, whose
reading is the ESP32 TRM's ("11.2.1 16-bit Prescaler and Clock Selection",
quoted inside esp-hal: 0 → 65536, 1 or 2 → 2, else the value). esp-hal
programs LACT with `divider = apb / 16_000_000` = 5 and divides the count by
16, so 80 MHz / 5 = 16 MHz and 16 ticks is one microsecond.

⚠️ `int_ena` **does not gate an interrupt on this chip** — esp-hal says so in
as many words and uses `tconfig.level_int_en` as the enable instead — so the
view drives the level sources from `int_raw & level_int_en` and keeps
`int_st = int_raw & int_ena` as the PAC defines it.

⚠️ **PD9**: `Instant::now()` is the guest reading a modelled counter. It is
self-consistent and deterministic; it is not a wall clock, and no host gate
runs on emulated microseconds.

### The RTC calibration, and the crystal

`rtccalicfg` counts XTAL cycles over `rtc_cali_max` cycles of the clock
`rtc_cali_clk_sel` picks. The view computes the measurement from modelled
clocks — XTAL 40 MHz (the desk board's crystal), RC_SLOW 150 kHz,
RC_FAST/256 = 31,250 Hz, XTAL32K 32,768 Hz, all from esp-hal's generated
clock tree — so on the direct load `detect_xtal_freq` reads 12,800 and
computes 40 MHz, `RTC_CNTL.store4` holds `0x0028_0028`, and
`calibrate_rtc_slow_clock` reads 273,066 and puts a real 150 kHz period in
`store1`. Before this phase all three were the 26 MHz / zero answers the
firmware's own timeout arm produced.

⚠️ The *duration* of a measurement is counted in **crystal** cycles rather
than `memmap::CPU_HZ`, because this machine has one guest-cycle rate for the
whole run while the guest has two (crystal inside `Clocks::init`, PLL after).
Completing early is unobservable — the driver delays and then polls;
completing late makes the driver time out and mis-detect the crystal, which
is exactly what P3 recorded. The full reasoning is on `Timg::cali_cycles`,
and the value the firmware consumes does not depend on it.

### The stall key is a pair, and the machine answers the question

A core is stalled only when `options0.sw_stall_*_c0` and
`sw_cpu_stall.sw_stall_*_c1` together read `(c1 << 2) | c0 == 0x86`
(`esp-hal-1.1.1/src/soc/esp32/cpu_control.rs:57-81`); `internal_park_core`
writes `c1 = 0x21` and then `c0 = 0x02`, so a model watching one register
would answer wrong through both halves of that sequence. `RTC_CNTL` computes
the pair and publishes it through `rtc_cntl::StallKey`, a shared cell the
machine holds, and `Machine::core_stalled` ORs it with its own field. **P4 added the third input**: DPORT's
`appcpu_ctrl_b.appcpu_clkgate_en`, `appcpu_ctrl_c.appcpu_runstall` and
`appcpu_ctrl_a.appcpu_resetting`, which esp-hal's `is_running` reads *before*
it looks at the RTC key at all (`cpu_control.rs:41-55`), published through
`dport::AppCoreControl`. Two blocks in two phases each own one input to one
question the machine answers, and neither phase reads the other's file.

## The CLI

*P2 skeleton, P6 doors, P8 completion.* `just emu-esp32v3 <elf>` is the
door; the recipe exists from P1 so the door has one name for its whole life.

```
--elf <path>            direct-load this image
--boot-mode direct|rom-up
--rom <path>            override the embedded mask ROM
--strict-bus            an access nothing claims is a STOP, not a zero
--cache-off-fetch stop|permit
                        D4; stop is the default and what every gate run uses
--core-quantum <cycles> the upper bound on one core's window (D3) [256]
--app-mmu-divergence stop|permit
                        R4; stop is the default and what every gate run uses
--time-grade t1         the only grade this machine defines
--timeout <5s|1500ms|900us>   EMULATED time
--wall-timeout <s>      the host-clock safety net; exits 4
--break-at <symbol>     stop at its first instruction
--probe <cycle>:<name>  print the word at a symbol at a guest cycle
--probe <name>@<ms>     the same, in EMULATED milliseconds — the C6 binary's
                        spelling, and the one `lp-emu-validate`'s payload
                        registry stores (M5 P1, ruling R2). Both forms work;
                        `@` decides when a value carries one
--trace <path|->  --trace-block <name>
--dump-frames <spec>    where decoded WS281x frames go: `-`/`stdout` or
                        `file:<path>`; one `ws281x-frame` JSON line per frame
                        (see "The pad"). Frames are kept in memory either way
--pin-log <path>        the raw edge stream, `<us> <pad> <level> cyc=<cycle>`,
                        with a `# route` note per routing change. Off by
                        default; capped at 2,000,000 lines
--strip-timing ws2812|ws2811    how every routed pad is decoded [ws2812]
--strip-order rgb|rbg|grb|gbr|brg|bgr
                        the order on the wire, which is what the record's
                        `rgb` field is unpermuted with [grb]
--uart0 <spec>          where UART0's bytes go: `-`/`stdout`, `memory`,
                        `file:<path>`, or `tcp:<addr>` to LISTEN for one
                        client at a time (whose bytes are UART0's RX). They
                        are always also kept in memory [memory]
--uart0-script <path>   deterministic host input on the wire
--uart0-baud <n>        the rate the host at the other end of the cable sends
                        at [115200] — what the auto-baud counters measure,
                        and nothing else
--control <tcp:addr>    LISTEN for a control-channel client: the cable's own
                        socket (see "The CH340 cable")
--control-script <path> the same verbs at declared EMULATED times
--reboot-on-reset       a release of EN reboots instead of ending the run
--exit-on <line>        stop at a COMPLETE line of UART0's output; exits 0
--efuse-mac <a:b:c:d:e:f>     the MAC the eFuse block answers [30:76:f5:ec:f6:34]
--efuse-rev <major.minor>     the chip revision it answers [3.1]
--flash <path>          back the flash chip with this file; written back at
                        the end of the run, and created blank if absent
--flash-copy <path>     read it once and never write it back
--merged <path>         --flash-copy, spelled for what a ROM-up boot wants:
                        the whole `espflash save-image --merge` chip. The
                        chip's length is taken from the file
--flash-len <bytes>     the chip's size [4194304, the desk board's 4 MB]
--seed <n>   --hooks   --map   --help
```

### The two sockets, and why there are two

A byte socket carries bytes and nothing else, because `lp-cli …
serial:tcp://` is a plain byte client and in-band control would be a dialect
every client would have to speak. So the wire is `--uart0` and the **cable**
is `--control`, a second socket in its own line protocol. Mixing them is an
error that names the other flag rather than a line quietly dropped.

`--uart0-script` and `--control-script` are the deterministic path: their
times are in the file, in emulated cycles, and two runs of one script deliver
the same bytes and the same verbs at the same cycles. A **live** socket lets
the host clock decide when a byte lands, so a run that must be a transcript
uses the script — *host time is not guest time*.

The byte script's grammar is the C6's: `<ms> "<string>"` or `<ms> <hex …>`
for absolute emulated time, `after "<line>" [+<ms>] <bytes>` for a host that
answers what it hears, and `then +<ms> <bytes>` for one pacing itself. `#`
starts a comment.

⚠️ `--timeout` is **emulated** time (PD9): no host gate runs on emulated
microseconds, and `--wall-timeout` is the separate wall-clock end.

⚠️ **An unrecognised flag is an error.** A door a phase has not opened is
deliberately *not* stubbed with a no-op, so the phase that adds it is visible
in the diff instead of silently changing what an old command line meant.
`--cache-off-fetch` arrived with P4; the six console-and-cable doors with P6;
`--flash`, `--flash-copy`, `--merged` and `--flash-len` with P7.

Exit codes are the C6's contract: 0 deadline **or an `--exit-on` match**,
2 fault **or a cable reset without `--reboot-on-reset`**, 3 strict-bus
refusal, 4 wall timeout, 5 `--break-at`; plus **6, a cache-off fetch**, which
extends the table rather than reusing one of its codes — a script that drives
both machines reads 0/2/3/4/5 the same way on either.

## Bring-up loop

P2 was the first phase that could run it; P3 ran it, and the commit history
of P3 is the log — one commit per block, each naming its stop.

1. `cargo run -p lp-emu-esp32v3 --release -- --elf <image> --strict-bus
   --timeout 300ms` (and `--boot-mode rom-up` for the other path).
2. Read the **first** stop. `Machine::first_strict_violation` names the
   earliest, and **the earliest strict stop is the root** — an exception after
   it is downstream and tells you nothing.
3. Classify it: an unmapped address in a declared window is a block to
   name; one outside every window is a memory-map question (P3's fifth stop
   was one, and the answer was a second bus); an unsupported opcode is an
   M1 finding; a tight spin on a register whose value no document gives is
   an **E-premise stop** — report it, do not invent a value.
4. Answer a block with an accept-and-remember `RegFile` seeded from the
   PAC, with any exception's evidence beside it: a ROM disassembly line, a
   PAC field, a linker constant, an esp-hal source line. Never "what the
   boot needed".
5. Run again, and record the cycle the run reached.

Without `--strict-bus` the run carries on with unmapped reads answering zero,
which is how far the machine gets before a block is modelled — useful for
scouting, never for a claim.

**The first three stops, as worked examples.** A bare machine
(`Esp32V3Builder::bare()`, P2's) still produces the first two.

```text
$ lp-emu-esp32v3 --boot-mode rom-up --strict-bus --timeout 50ms      (bare)
STRICT BUS STOP
  pc      = 0x4000fdd8 (~_rtc_trigger_sw_system_reset+0x11)
  cycle   = 7 (0 us emulated)
  access  = Read Word at 0x3ff5a000
```

`0x3FF5_A000` is **EFUSE** `blk0_rdata0`, and the pc is inside
`_ResetHandler_efuse_check_patch` (`0x4000_FDA0` — the tilde means "nearest
preceding label"; this ROM's labels are mostly zero-sized). Seven
instructions after the reset vector the ROM reads its own fuses. Answer:
`accept::efuse()`, the PAC's zeros. The next reading of the ROM path is not
a strict stop but a spin — `_reload_efuses_and_check` writes `cmd.read_cmd`
and needs it to read 1, then 0 — and that is where the ROM path stands
until P5 models the eFuse controller's completion.

```text
$ lp-emu-esp32v3 --elf …/fw-esp32v3 --strict-bus --timeout 50ms      (bare)
STRICT BUS STOP
  pc      = 0x40125775 (esp_hal::soc::xtensa::esp32_init+0x175)
  cycle   = 29 (0 us emulated)
  access  = Write Word at 0x3ff00218
```

`0x3FF0_0218` is **DPORT** `core_1_intr_map[0]`: twenty-nine instructions
in, `esp_hal::init` starts clearing the APP core's interrupt map. Answer:
`accept::dport()`, the PAC's 49 non-zero resets, no exception.

```text
$ lp-emu-esp32v3 --elf …/fw-esp32v3 --strict-bus --timeout 50ms   (DPORT accepted)
STRICT BUS STOP
  pc      = 0x400081df (rtc_get_reset_reason+0xb)
  cycle   = 107539 (448 us emulated)
  access  = Read Word at 0x3ff48034
```

`0x3FF4_8034` is **RTC_CNTL** `reset_state`, read by the mask ROM for
`esp_hal::rtc_cntl::reset_reason`. Answer: `accept::rtc_cntl(cause)` — and
the one deviation from the PAC in the whole set, because the PAC's reset
has both cause fields zero and a chip that just powered on reads
`POWERON_RESET` (1) in each; the ROM's `extui` masks and the silicon banner
are the citation.

**Where the loop ended.** P3 recorded eleven stops and left the direct load
spinning on `SPI1.cmd.flash_rdsr` (P7's) with the ROM path on
`EFUSE.cmd.read_cmd` (P5's). The last of them was **RMT**, met by both paths
one line after `[INIT] flash filesystem mounted` — `init_board`'s
`Channel::new` reading `ch0conf1` at cycle 5,640,047 on the direct load and
65,360,003 on the ROM-up walk. It was an accept block through M3; **M4 P2
replaced it with the view** (“RMT on the classic” above), and behind it both
paths still idle — the shipped image configures its four slots and transmits
nothing until a project's output opens. The full ledger, with every pin's citation and the order
it fixed for P4–P8, is
`docs/reports/2026-09-10-esp32v3-strict-boot-inventory.md`.

## Tests

| Test | What it holds |
|---|---|
| `tests/memmap.rs` | No two declared regions overlap; every `periph::*` base is inside the MMIO window; `dram_seg` is 8 KiB above the ROM's reserve with `RESERVE_DRAM = 0`; the vector table is 1 KiB below the IRAM; SRAM0 is one region containing the bootloader's `0x4007_8000`; the SRAM1 I-bus alias is named and unmapped; the ROM extents match the vendored ELF; 240 cycles to the microsecond |
| `tests/rom_vendoring.rs` | The embedded ROM's sha256, re-derived in-process from `rom::VENDORED_V3_ROM` itself, is the one `SHA256SUMS` records — and the file on disk is still that file |
| `tests/boot.rs` | 39 `PT_LOAD`s, thirteen empty and counted, the ELF-header-mapping segment recognised, four relocated segments placed by vaddr; the ten vector sections at their documented `VECOFS`; at least eight non-alloc sections seeded with real bytes, `.data_xtos_pro` among them; `break 1, 15` matching `lp_xt_inst::encode`; the hook table shipping empty; two hart slots with slot 1 stalled and taking no cycles; `PS_BOOT` after a direct seed; the boot frame's spill target mapped where an unseeded one is not; a seeded hart surviving a real exception; a strict rom-up run on a **bare** machine stopping inside the MMIO window; the boot set registered in the declared order; the ROM path standing at `EFUSE.cmd`; snapshot round-trip. **With the image** (`just test-emu-esp32v3-boot`): `.data` a self-copy and placed; `.rtc_fast.persistent` the one relocated segment; the flash chip seeded over the ROM's 2 MiB default through `spi_w25q16`; the hart entered at `Reset` with the bootloader's `a1`; P2's first stop held on a bare machine; the `[INIT]` chain read back out of the UART0 trace and the run standing at `SPI1.cmd`; two runs identical |
| `tests/clock.rs` | LACT counting at its derived 16 MHz and latching rather than reading live; TIMG0's three counters independent; the classic's interrupt registers at `0x98..0xa4` and the level source driven from `level_int_en`; the MWDT dropping a write without its key; the calibration answering 12,800 for `detect_xtal_freq` (40 MHz) and 273,066 for the slow clock (150 kHz); TIMG1 at sources 18..21; the block at the PAC's resets; state round-trip. **Through the machine**: the reset cause in both fields; the stall key reaching `Machine::core_stalled` only when *both* halves are written; the desk board's MAC and v3.1 out of the eFuse words plus `APB_CTRL.date`; a run in which no watchdog fires. Plus an `#[ignore]`d measurement of what a LACT timestamp costs |
| `src/periph/uart.rs` (unit) | the reset state as the ROM console's 115,273 baud with `clkdiv` = the PAC's `0x2b6` and `status` the PAC's zero; the 921,600 divisor the image programs and its 2,610-cycle symbol; a byte leaving the wire one symbol after the FIFO write; **`status` bit 23 set at 128 queued bytes — the bit the ROM's `uart_tx_one_char` spins on**; esp-hal's own `rx_fifo_count` formula over `mem_rx_status` answering the true count; levels versus sticky; the timeout's empty-FIFO rule; `conf0`'s FIFO-reset bits at 17/18 and not the C6's 22/23; `tick_ref_always_on` moving the block onto REF_TICK; the auto-baud counters measuring the stated host rate; state round-trip; UART1 at its own source; and **no register in the block graded `measured`** |
| `src/control.rs` (unit) | the auto-reset truth table as a table; every verb parsing to its command; the three verbs this chip does **not** have refused by name; the reply formats; each script refusing the other one's lines |
| `src/periph/*.rs` (unit) | every accept block reads the PAC's resets except the listed deviation, and every listed deviation is real and has a reason; the eFuse read command reading set exactly once and then clearing itself, and three reloads comparing equal; RTC_CNTL's stall pair, its RWDT gate and its reported-not-performed software reset; the analog master answering the register asked for rather than the last one written |

| `tests/boot_idle.rs` | **The hello, and G2 (a).** With the image: the `[INIT]` chain comes out of the **host stream** — 543 bytes, sha256 `ea8bae30…`, the same count and the same digest P3 measured going *into* the accept block — with zero unmapped accesses and no strict stop; the line order held against L0's capture; `--exit-on` stopping at a complete line. And both boot paths run to the **idle heartbeat**: the `[stack] heartbeat:`/`[MEM]`/`[JIT]` triple, the Q5 fallback line, the unsolicited wire hello, the answer to a scripted request, and `unmapped = 0` on each — plus the two paths' memory figures asserted equal to **each other** (not to silicon: different image bytes, ruling R7) |
| `tests/rmt_registers.rs` | **M4 P2's gates.** Every offset looked up **by name** in `regs::RMT` rather than transcribed, and the resets asserted against that table's own `resets`. One test per quirk, each of which fails without it: `tx_lim` a repeating count that re-arms itself (one write, a 256-word transmission, four events 64 words apart — the C6's position semantics fire once); a `ch_tx_lim` write changing the period and not the count; the **global** wrap bit, and the same register serving channel 7; `tx_start` acted on with no `conf_update`; a window of end markers stopping the transmitter at the next word boundary; `mem_raddr_ex` absolute **and at bits 12:21**, with the APB write pointer at 0:9. Plus: a whole **WS2812 frame** — 8 LEDs, 194 words, a 128-word window, three ping-pong refills — driven by the register sequence the shipped image's own `--trace-block RMT` run issues, whose fetched words are the stream that went in, whose word start cycles are exact absolute tick positions, and whose edges off **gpio18** decode back to the bytes; `Gpio::peripheral_driven_pads` non-empty for the first time; the refill telemetry's entry and fill in words; `int_clr`'s W1C and the line on source 47; REF_TICK refused rather than guessed at; a byte-identical snapshot round trip; and **no register in the block graded `measured`** |
| `tests/pin_frames.rs` | **M4 P3's gates.** The WS281x decoders on the whole machine: a pad becomes observed when the guest routes it, and `routed_pads()` carries `(PadId(18), RMT_SIG_0)`; the engine ends a transmission; the frame decoded off the fabric is whole, zero-error, 24 bytes, closed by its reset, and **is the frame that went in**; a frame the run ended mid-flight is flushed **incomplete** rather than invented; `--dump-frames` writes one `ws281x-frame` line naming `RMT_SIG_0` and carrying `wire` and `rgb` as two different fields; `--pin-log` writes one line per edge in guest-cycle order with a `# route` note; two runs write byte-identical dumps and so does a third at a different `--core-quantum`; and a decoder snapshotted **mid-bit** restores with its half-shifted bits and decodes the second half identically. Driven by the shipped image's own register sequence from the host side, because R6 blocks the upload that would make the guest issue it |
| `tests/rmt_chase.rs` | **M4 P5's gate.** The `rmt-chase` payload's own image run whole to its done marker under `--strict-bus`, and **all 768 frames the decoder read off IO18 checksum-equal to the guest's own `rmt-frame` record** — `fnv1a(unpermute(frame.wire, order))` against the line the firmware printed — with zero bit errors, zero trailing bits, 6,144 bits and 256 LEDs a frame, a reset gap closing every one and the chase pixel where the record says it is. Plus: IO18's route present and `RMT_SIG_0` with no *other* RMT signal routed anywhere (**not** the length of `routed_pads()` — gpio1, the console pad, is in it); two edges per bit and no dropped edges; two runs writing byte-identical dumps and a decoder snapshotted **mid-bit** restoring to the same frames, both over a 60 ms prefix because a whole run is 3.33 billion cycles. The first **guest-driven** channel on this chip — see "`rmt-chase` on the classic" |
| `tests/shader_oracle_pin.rs` | **M4 P4's gate: a frame three ways.** `walks/shader-oracle.script` — a real `lp-cli upload` replayed in guest time over UART0 — on the **`frame-dump`** image, then the first *lit* frame off IO18 against the host oracle pinned from a run in this tree (`ORACLE_RGB`, `ORACLE_CRC = 0x5577_2254`, both engines agreeing on all 192 bytes) and against the firmware's own deferred `[OUT] dump` line; every later frame the same frame; the open line with `gpio=/gpio/18`; `routed_pads()` carrying `(PadId(18), RMT_SIG_0)` *by value*; `[INIT] RMT ISR on APP core` as P1's standing guard; two runs' `--dump-frames` sha256-equal. Plus the script's own shape, the FNV-1a vectors and the oracle constant's length and checksum, which need no firmware. All of it runs since M4 P4b (#711) — see "A frame three ways" |
| `tests/five_wires.rs` | **M4 P4's second wave.** `projects/test/five-wire` uploaded by `walks/five-wire.script`: five pads routed, four pooled two-block slots, and **one signal driving a second pad with a park to `GPIO_OUT` between** — the re-mux, read off the pin log's routing notes; whole frames with no bit errors on every wire; the same frames across two runs and across two `--core-quantum` values (by *shape*: the pusher is on core 1, so the starts move by a window while the bytes do not). Plus, since M4 P4b (#711), the per-wire checksum against the guest's own `[OUT] frame=… crc=` summary lines — five wires, five distinct byte strings, each one a checksum the guest printed — and no frame dropped |
| `tests/determinism.rs` | The plan's inviolable invariant. Two runs of each boot path agree on the UART sha, the byte count, the **cycle count**, the **instruction count**, the pc and the idle skips; a snapshot taken mid-run and restored into a **fresh** machine produces the same second half as the run that was never interrupted; and the state that is not a register — the flash MMU tables, the cache-enable bit, both halves of the stall key, the interrupt matrix, core 1's hold, the DBREAK slots — comes back through the struct. **M4 P1**: the single-core prefix run's three counters pinned to `origin/main`'s; two dual-core runs at quantum 256 and two at 64 each one run (both harts' counters, both pcs, the parks, a memory fingerprint); the two quanta's consoles equal byte for byte except the stack high-water figure, which the test names as interrupt timing |
| `tests/dual_core.rs` | **M4 P1's gates.** A hand-built fixture with no firmware: core 0 performs esp-hal's `start_core1` DPORT sequence, core 1 comes up **through the mask ROM's own reset path and wait loop** (its two reads of `appcpu_boot_addr` read out of the bus trace), routes the doorbell into its own matrix and parks in `waiti` — costing nothing while parked — and core 0's `cpu_intr_from_cpu_1` wakes it into `_Level2InterruptVector` with `EPC2` naming the instruction after the `waiti`; the release is a reset (`CPENABLE = 0xff`, counters from the clock). Ruling R4 on a synthetic table disagreement: the stop names entry, page and both mappings, exits 7, and `permit` continues. **With the image**: `[INIT] RMT ISR on APP core` on both boot paths with `unmapped = 0`, the binds read back out of `core_1_intr_map` with `core_0_intr_map[RMT] = 16`, the pusher parked in `idle_once` — **red on the shipped image until the open defect below is fixed** |
| `src/periph/gpio.rs`, `src/periph/io_mux.rs` (unit) | A plain `Output` pin drive reaching a pad and `enable` taking it back off the wire; `256` being the GPIO selector and `128` an ordinary signal; bank 1 carrying pads 32..39; the input matrix routing `U0RXD_IN` and refusing the two constants by name; `in_` served only through `fun_ie`; the PRO core's enable at `pin[n]` bit 15 and the APP core's at 13; **no peripheral signal reaching a pad**; the IO_MUX pad map walked against the generated table's own names, and asserted *not* to be in pad order |
| `tests/uart_socket.rs` | The view **through the bus**, at the addresses a guest uses: scripted bytes arriving at the cycles the file names and reading back in order; an `int_clr` unable to clear `rxfifo_full` while it holds; the receive timeout refusing to clear until the FIFO is empty (the classic's third category). And the **cable** at machine level: the reboot on the *release* of EN and not on the assert, `reset` and `download-mode` one reboot each with the right strap, a release without `--reboot-on-reset` ending the run and naming the strap, and `attach`/`open` moving no chip state. With the image: a cable reset of the running app, one reboot, and **both boots in one console log** — 1,086 bytes, two identical halves |

Run them with `just test-emu-esp32v3`, and the image-backed ones with
`just test-emu-esp32v3-boot`. **`just test-emu-esp32v3-gate` is the whole of
M3's gate** — that suite, both lints, and the reference image built twice for
`--verify` — and it is what CI's path-gated, non-required
`Emulator ESP32v3 (x64)` job runs.

## The reference image

`scripts/emu/build-reference-image.sh --chip esp32 esp32,server,float-f32
<commit> none` builds the shipped image in a detached worktree at a pinned
commit, so `build.rs`'s `git rev-parse` stamps the right commit and the right
`dirty` flag. `--verify` builds it a second time in a second cold worktree at
a longer path and fails if the sha256s differ.

**There is no spike feature and no memfs variant.** The C6 cherry-picks
`spike_uart0_link` because its host link is USB-Serial-JTAG; the classic's
link *is* UART0, so the tree stays clean and `dirty: false` is the honest
stamp — the same stamp a silicon flash of that commit carries. And the
classic boots from a modelled flash chip with a real filesystem, which is
what "memfs-free" means in G2.

**The per-host band, measured** (ruling R6): on an M2 Max with `rustc
1.97.0-nightly (ca9a134e0)` from the `esp` channel and `xtensa-esp-elf`
`esp-14.2.0_20240906`, the image at `a795b664f` is 2,985,952 bytes, sha256
`e4c41e7e…`, twice. The band is *one host, one sha*; cross-host equality is a
further claim and is not gated on.

⚠️ **A fourth cause of non-reproducibility, which the C6's header does not
list**: a `~/.cargo/config.toml` with
`build-dir = ~/.cache/cargo-build/{workspace-path-hash}` puts a hash of the
workspace path into the ELF's debug info through the build scripts'
`OUT_DIR`. Two pinned worktrees produced images differing in exactly sixteen
bytes. Remapping the parent cannot help — the hash is *inside* the path — so
the recipe pins the build dir under the worktree, which the existing remap
already covers.

⚠️ **L0's desk board is running a dirty tree** (`2e21b6226bcd`-dirty), so no
commit rebuilds the bytes the first classic silicon transcripts came from.
That is ruling **R7**, it is a director decision before lab task L1 is
dispatched, and it is why G2's memory comparison against silicon is held:
the boot-log *shape* is comparable line for line, and the memory-class fields
are comparable only against an emulated run of the same bytes.

## Provenance

- The mask ROM is Apache-2.0, from `espressif/esp-rom-elfs` release
  `20260528`. It is committed **verbatim**; see `../roms/README.md`.
- The register-name tables in `src/regs/` are **generated** from the `esp32`
  PAC's svd2rust offset comments by `scripts/emu/pac-regnames.py --pac esp32`
  and carry the provenance header
  `docs/adr/2026-07-29-license-provenance-discipline.md` requires. Never
  hand-edit one; `just lint-emu-regnames` catches it, for both chips.
- `RNG` **is** generated, since P3. P1 had excluded it — `esp32-0.40.2/
  src/lib.rs:647` gives it base `0x6003_5000`, which is not in the DPORT
  window — as an SVD leak; P3 found the AHB bus, where `0x6003_5000 +
  0x144` is the classic's `WDEV_RND_REG`. See "The memory map".
- Every constant in `memmap.rs` carries the `file:line` it was read from.
- The crate is MIT, as a unit with the rest of `lp-emu/`. See
  `../../README.md` and `just lint-emu-fence`.

## `rmt-chase` on the classic

*M4 P5. The payload's classic harness, the gate, and the pin transcript.*

`rmt-chase` is a 256-LED white dot walking a strip three times — **768
frames** — with one line per frame saying what the guest believes it sent:

```text
[fw-check-json] {"kind":"rmt-frame","n":0,"leds":256,"lit":1,"crc":"0xf88210a7"}
```

The pattern, the FNV-1a checksum, the record and the done marker are
`fw-checks`'s (`checks::rmt_chase`) and are host-tested; the chip half is
`lp-fw/fw-esp32v3/src/tests/test_rmt.rs`, behind `--features esp32,test_rmt`.
That half drives the **product's own backend** — `shared_driver::DRIVER` over
`v3_rmt`, opened with the same register sequence `Esp32V3RmtWs281xDriver::new`
uses and routed with the same `route_rmt_to_gpio` the slot pool uses — rather
than a ported wrapper (ruling R5; the classic has no `output::LedChannel`).
It never starts the APP core: the payload is meant to be the simplest frame
path this chip has, and a second core would add the wire pusher's slot pooling
to it.

### The three readings, and how many of them exist today

| reading | where it comes from | today |
|---|---|---|
| the guest's own record | `rmt-frame` lines over UART0 | **here** |
| the decoder's frame | the pad, `--dump-frames` | **here** |
| silicon's | an instrument, or the same payload flashed and captured | **M5's** |

> **Today both readings are ours.** The decoder is this repository's, the
> fabric is this repository's and the RMT model is this repository's, so what
> the gate below shows is that a bug would have to be in the same place in
> three independent code paths to hide. **The silicon twin is M5's**, and it is
> the only thing that turns any of this into a measurement.

### The gate

`tests/rmt_chase.rs`, run by `just test-emu-esp32v3-boot` against the image
that recipe builds (`LP_EMU_ESP32V3_TEST_RMT_ELF` — a **second** variable,
because every feature set builds to one target path and a gate that read
whatever was there last would compare the chase against the shipped image):

1. the payload run whole to its own done marker under `--strict-bus`;
2. **all 768 frames checksum-equal** — `fnv1a(unpermute(frame.wire, order))`
   against the record's `crc` — with zero bit errors, zero trailing bits, a
   reset gap closing every frame, and the chase pixel where the record says;
3. IO18's route present and `RMT_SIG_0` (`out_sel = 87`), and no *other* RMT
   signal routed anywhere;
4. two runs writing byte-identical `--dump-frames` files, and a decoder
   snapshotted **mid-bit** restoring to decode the same frames.

⚠️ **Not "exactly one routing note".** A plain boot routes **gpio1** too — the
console TX pad, through `func_out_sel_cfg` — so it gets a decoder and a
`pin gpio1: 0 frames …` summary line of its own. Asserting the length of
`routed_pads()` would fail on a fact about the console. What is asserted is
IO18's route *by value*, and that one RMT signal is routed.

⚠️ **`unpermute`, where the C6's twin checksums the wire bytes directly.** The
C6's `LedChannel` swaps RGB→GRB and `lp-ws281x` then permutes again, so the
C6's wire carries the caller's RGB (`fw-checks`'s double-swap finding, DD34
d). The classic harness hands the driver RGB and `lp-ws281x` permutes
**once**, so this wire carries real GRB. This payload cannot tell the
difference — every pixel is grey, and `wire == rgb` on all 768 frames — which
is exactly why the correct form is written rather than the one that happens to
pass.

### Cost, and what the gate runs whole

768 frames at 18,067 µs is **13.885 s of guest time, 3.33 billion cycles,
about 107 s of host time**. The checksum gate runs the payload whole, because
that is the claim; the determinism and snapshot gates run a 60 ms prefix of
the same run rather than paying for two more, and say so.

### The transcript

`lp-emu/transcripts/esp32v3/rmt-chase/lp-emu-esp32v3-t1-2026-09-11-dc2df69d4.txt`
— the console — beside its `.pins.jsonl` (transcript **shape B**: one
`ws281x-frame` record per frame) and a sidecar naming the image commit and
sha256, the features, the pin companion and the command that produced them.
Hand-run and flagged as such: `lp-cli validate` has no `rmt-chase` arm for
this chip, because the registry and the configuration are **M5's**.

⚠️ **The facts about the run itself are in the sidecar's `note`, not in
fields of their own.** The grade (`t1`), the **core quantum** (256), the boot
path (direct), the core count and the run's counters — cycles, instructions,
frames, edges, refills — are one labelled line each in the last paragraph of
`note`, under `boot path:`, `time grade:`, `core quantum:`, `cores:` and
`run:`. `TranscriptHeader` is `deny_unknown_fields` (M5 ruling R3), so a
sidecar that spelled them as top-level keys is *refused*, loudly, by
`every_committed_transcript_is_filed_where_its_header_says`. Widening the
header is the contract's business and belongs to **M5 P6**, the replay
phase; this phase records an artefact. When M5 P6 promotes them to fields it
can lift them straight out of those lines.

**Never edit a transcript.** A mismatch is a regression or a re-capture.

### What P5 found that P3 could not

P3's waveform was driven from the **host** (`tests/pin_frames.rs` writes the
shipped image's own register sequence through the bus) because the shipped
image starts no channel until a project's output opens — ruling **R6**. This
is the first **guest-driven** channel on the classic: the firmware opens it,
starts all 768 frames and services all **36,864** refills from its own ISR.
Nothing behaved differently for it. The refill telemetry sits in bucket 0 for
both entry and fill on every one of the 36,864 — entry never worse than 2
words, fill never worse than 12, against a 128-word half — there are exactly
two edges per bit and no dropped edges, and no frame truncated. A refill
racing `tx_end` under guest timing was the thing to watch for and it did not
happen.
