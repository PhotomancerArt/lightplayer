# `lp-emu-esp32v3` — the classic ESP32 (v3) machine

This is where the classic's chip numbers live. `lp-emu-esp-common` supplies
the bus, the peripheral model, the trace and the ELF view and knows nothing
about any chip; `lp-xt-emu` supplies the Xtensa hart and knows nothing about
MMIO. Here they are put together with a memory map, a mask ROM, a reset state
and a run loop, and the result takes a `fw-esp32v3` binary.

It is `lp-emu-esp32c6`'s twin, deliberately: same shape, same module names,
different silicon.

> **M3 P3.** What exists today is the memory map (both peripheral buses),
> the generated register-name tables, the vendored mask ROM, the bus, the
> two-slot machine, the run loop, the snapshot, the direct load, and
> **thirteen accept-and-remember blocks** — the ones the strict bring-up
> pass demanded, in the order it met them. **No peripheral has behaviour.**
> The direct load prints its whole `[INIT]` chain into an accept block and
> spins on the flash controller; the ROM path spins on the eFuse read
> command. The sections below that name a later phase are stubs, and they
> say so rather than describing a machine that does not exist yet. The stop
> ledger is `docs/reports/2026-09-10-esp32v3-strict-boot-inventory.md`.

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

On silicon a stalled core is held by three inputs, and `Machine::core_stalled`
is the OR of them. **P5 wired the first two**:
`RTC_CNTL.options0.sw_stall_appcpu_c0` plus
`RTC_CNTL.sw_cpu_stall.sw_stall_appcpu_c1` — both halves of one key, which
stalls only when `(c1 << 2) | c0 == 0x86` — computed by the block that owns
them and published through `rtc_cntl::StallKey`, a shared cell the machine
holds. **P4 adds the third**, `DPORT.appcpu_ctrl_c.appcpu_runstall`, into the
same OR. The machine's own field still holds slot 1 for the whole of M3.

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
table or image validation; (2) no flash MMU programming — in P3 the flash
windows are plain RAM with no chip behind them; (3) the ROM console never
initialised; (4) no ROM banner, no bootloader log; (5) no early RNG entropy;
(6) eFuse asserted, not read; (7) the reset cause asserted as POWERON; (8)
`.data` placed rather than copied; (9) core 1 stalled by assertion, not by
the ROM's `sw_stall` path; (10) `chip_size` written by the loader in place
of `esp_rom_spiflash_config_param`; (11) the cache MMU "left enabled" by
there being no cache model at all — the state D4's stop (P4) is defined
against.

`just test-emu-esp32v3-boot` builds the shipped image and runs the
direct-load tests against the file it built (`LP_EMU_ESP32V3_ELF`; see
`src/test_support.rs` for why the conventional target path is never trusted
from inside a test).

## Booting from the reset vector

*P5 opened it; P7 finishes it.* P3 left a `--boot-mode rom-up --strict-bus`
run spinning seven instructions after the reset vector, inside the ROM's
anti-glitch check on its own fuses. With `periph::efuse` making the read
command a completion the check passes, the ROM walks into `main`, and the run
now stops at `uartAttach+0x43` (`0x4000_9013`) writing `UART1 +0x10` at cycle
9,488 — the block P3's §4.3 named next.

*P7.* The mask ROM boots the espflash-merged image through the real IDF
bootloader, and the boot log is compared line for line against silicon.

## The CH340 cable

*P6.* On the classic the port is a **bridge chip on the board**, not a
peripheral inside the SoC, so opening the port moves no chip state: what
resets the chip is the auto-reset circuit driven by the modem lines, and the
truth table is the board's, not the chip's.

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

The window itself is served as a **cache fill**, not per-access translation
(P7): instruction fetch stays a RAM read, which is what keeps the machine
usable, and the model is stricter than silicon about staleness.

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

*P4 and P5 between them gave six blocks behaviour; the rest are accept probes
until P6–P8.* One row per block, in registration order
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
| `GPIO` | `0x3FF4_4000` | `0x600` | accept | direct #7, cycle 3,564,113 | the matrix routing U0RXD; `strap` reads the PAC's 0 | P8 |
| `UART0` | `0x3FF4_0000` | `0x80` | accept | direct #8, cycle 3,564,269 | `Uart::new` at 921600; **every printed byte lands here and is forgotten** | P6 |
| `IO_MUX` | `0x3FF4_9000` | `0x94` | accept | direct #9, cycle 3,568,671 | the U0TXD pad | P8 |
| `SPI1` | `0x3FF4_2000` | `0x400` | accept | direct #10, cycle 3,644,210 | esp-storage's flash read; **the run spins on `cmd.flash_rdsr`** | P7 |
| `SPI0` | `0x3FF4_3000` | `0x400` | accept | direct #11, cycle 3,644,282 | the ROM's idle wait on `ext2.st` | P7 |
| `EFUSE` | `0x3FF5_A000` | `0x200` | **view** | rom-up #1, cycle 7 | the fuse array, the read-data registers, and the read command as a **completion**; the MAC and chip revision | P5 |
| `FLASH_MMU` | `0x3FF1_0000` | `0x4000` | not reached by P3 | **view** | the two flash MMU page tables, written by the ROM's `mmu_init` and `cache_flash_mmu_set`; the fill is P7's | P4 / P7 |

The two deviations from the PAC, both inputs to the run rather than
properties of the part:

- `RTC_CNTL.reset_state` — POWERON_RESET in both cause fields, the value
  L0's banner printed (`rtc_cntl::DEVIATIONS`);
- `APB_CTRL.date` bit 31 — esp-hal's `eco_bit2`, the top bit of the major
  chip revision, which no eFuse word on this part carries
  (`accept::DEVIATIONS`).

Grades: every register is `documented` where the PAC calls it read-write and
the block pretends nothing, `modeled` otherwise (`RegFile::with_pac_grades`);
the analog master, which the PAC does not know, and every register of the
eFuse view are `modeled` throughout. Nothing is `measured`.

Not modelled, on purpose, and unmapped so a strict run says so: everything
neither boot has reached — `UART1` (where the ROM-up path now stands),
`RTC_IO`, `SENS`, `RTC_I2C`, `FRC_TIMER`, `FLASH_ENCRYPTION`, `SHA`, `RMT`,
`RNG` and the WiFi window. Their **AHB mirrors** are unmapped for the same
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

*P2 skeleton, P8 completion.* `just emu-esp32v3 <elf>` is the door; the recipe
exists from P1 so the door has one name for its whole life.

```
--elf <path>            direct-load this image
--boot-mode direct|rom-up
--rom <path>            override the embedded mask ROM
--strict-bus            an access nothing claims is a STOP, not a zero
--cache-off-fetch stop|permit
                        D4; stop is the default and what every gate run uses
--time-grade t1         the only grade this machine defines
--timeout <5s|1500ms|900us>   EMULATED time
--wall-timeout <s>      the host-clock safety net; exits 4
--break-at <symbol>     stop at its first instruction
--probe <cycle>:<name>  print the word at a symbol at a guest cycle
--trace <path|->  --trace-block <name>
--efuse-mac <a:b:c:d:e:f>     the MAC the eFuse block answers [30:76:f5:ec:f6:34]
--efuse-rev <major.minor>     the chip revision it answers [3.1]
--seed <n>   --hooks   --map   --help
```

⚠️ `--timeout` is **emulated** time (PD9): no host gate runs on emulated
microseconds, and `--wall-timeout` is the separate wall-clock end.

⚠️ **An unrecognised flag is an error.** The doors P6/P7/P8 add (`--uart0`,
`--uart0-script`, `--control`, `--flash`) are deliberately *not* stubbed with
no-ops, so the phase that adds one is visible in the diff instead of silently
changing what an old command line meant. `--cache-off-fetch` arrived with P4.

Exit codes are the C6's contract: 0 deadline, 2 fault, 3 strict-bus refusal,
4 wall timeout, 5 `--break-at`; plus **6, a cache-off fetch**, which extends
the table rather than reusing one of its codes — a script that drives both
machines reads 0/2/3/4/5 the same way on either.

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

**Where P3 leaves it.** The direct load runs eleven stops deep, prints its
whole `[INIT]` chain into `UART0.fifo` (543 bytes, cycles 3,622,541 to
3,642,925, the silicon capture's lines as far as `[INIT] I/O task
spawned`) and then spins in `esp_rom_spiflash_read_status` on
`SPI1.cmd.flash_rdsr` — the phase file's own example of a stop that names
its owner, P7. The ROM path spins on `EFUSE.cmd.read_cmd` — P5. Both
readings are pinned in `tests/boot.rs`, along with the determinism of the
run. The full ledger, with every pin's citation and the order it fixes for
P4–P7, is `docs/reports/2026-09-10-esp32v3-strict-boot-inventory.md`.

## Tests

| Test | What it holds |
|---|---|
| `tests/memmap.rs` | No two declared regions overlap; every `periph::*` base is inside the MMIO window; `dram_seg` is 8 KiB above the ROM's reserve with `RESERVE_DRAM = 0`; the vector table is 1 KiB below the IRAM; SRAM0 is one region containing the bootloader's `0x4007_8000`; the SRAM1 I-bus alias is named and unmapped; the ROM extents match the vendored ELF; 240 cycles to the microsecond |
| `tests/rom_vendoring.rs` | The embedded ROM's sha256, re-derived in-process from `rom::VENDORED_V3_ROM` itself, is the one `SHA256SUMS` records — and the file on disk is still that file |
| `tests/boot.rs` | 39 `PT_LOAD`s, thirteen empty and counted, the ELF-header-mapping segment recognised, four relocated segments placed by vaddr; the ten vector sections at their documented `VECOFS`; at least eight non-alloc sections seeded with real bytes, `.data_xtos_pro` among them; `break 1, 15` matching `lp_xt_inst::encode`; the hook table shipping empty; two hart slots with slot 1 stalled and taking no cycles; `PS_BOOT` after a direct seed; the boot frame's spill target mapped where an unseeded one is not; a seeded hart surviving a real exception; a strict rom-up run on a **bare** machine stopping inside the MMIO window; the boot set registered in the declared order; the ROM path standing at `EFUSE.cmd`; snapshot round-trip. **With the image** (`just test-emu-esp32v3-boot`): `.data` a self-copy and placed; `.rtc_fast.persistent` the one relocated segment; the flash chip seeded over the ROM's 2 MiB default through `spi_w25q16`; the hart entered at `Reset` with the bootloader's `a1`; P2's first stop held on a bare machine; the `[INIT]` chain read back out of the UART0 trace and the run standing at `SPI1.cmd`; two runs identical |
| `tests/clock.rs` | LACT counting at its derived 16 MHz and latching rather than reading live; TIMG0's three counters independent; the classic's interrupt registers at `0x98..0xa4` and the level source driven from `level_int_en`; the MWDT dropping a write without its key; the calibration answering 12,800 for `detect_xtal_freq` (40 MHz) and 273,066 for the slow clock (150 kHz); TIMG1 at sources 18..21; the block at the PAC's resets; state round-trip. **Through the machine**: the reset cause in both fields; the stall key reaching `Machine::core_stalled` only when *both* halves are written; the desk board's MAC and v3.1 out of the eFuse words plus `APB_CTRL.date`; a run in which no watchdog fires. Plus an `#[ignore]`d measurement of what a LACT timestamp costs |
| `src/periph/*.rs` (unit) | every accept block reads the PAC's resets except the listed deviation, and every listed deviation is real and has a reason; the eFuse read command reading set exactly once and then clearing itself, and three reloads comparing equal; RTC_CNTL's stall pair, its RWDT gate and its reported-not-performed software reset; the analog master answering the register asked for rather than the last one written |

Run them with `just test-emu-esp32v3`, and the image-backed ones with
`just test-emu-esp32v3-boot`.

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
