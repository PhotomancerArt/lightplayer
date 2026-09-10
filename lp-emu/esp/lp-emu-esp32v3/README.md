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

*P3: accept blocks only; P4–P8 give them behaviour.* One row per block, in
registration order (`machine::PERIPHERAL_REGISTRATION_ORDER`, which is the
order the boot met them). Every block is a `RegFile` seeded from the PAC's
reset values (`src/periph/accept.rs`); **one** register deviates from the
PAC and the test `the_only_deviations_from_the_pacs_resets_are_the_listed_ones`
is the list.

| block | base | aperture | stop | what it answers here | owner |
|---|---|---|---|---|---|
| `DPORT` | `0x3FF0_0000` | `0x1000` | direct #1, cycle 29 | interrupt maps, clock/reset gates, `cpu_per_conf` — written, read back | P4 |
| `RTC_CNTL` | `0x3FF4_8000` | `0x140` | direct #2, cycle 107,539 | `reset_state` = POWERON in both fields (**the deviation**); RWDT disable | P5 |
| `APB_CTRL` | `0x3FF6_6000` | `0x80` | direct #3, cycle 109,663 | `sysclk_conf.pre_div_cnt`, the tick confs | P5 |
| `TIMG0` | `0x3FF5_F000` | `0x100` | direct #4, cycle 109,989 | MWDT disable; the RTC calibration **times out** (no pretence) | P5 |
| `I2C_ANA_MST` | `0x6000_E000` (AHB) | `0x20` | direct #5, cycle 143,014 | BBPLL writes through the ROM's `rom_i2c_writeReg`; busy is a bit the guest never sets | P5 |
| `TIMG1` | `0x3FF6_0000` | `0x100` | direct #6, cycle 3,563,841 | MWDT disable | P5 |
| `GPIO` | `0x3FF4_4000` | `0x600` | direct #7, cycle 3,564,113 | the matrix routing U0RXD; `strap` reads the PAC's 0 | P8 |
| `UART0` | `0x3FF4_0000` | `0x80` | direct #8, cycle 3,564,269 | `Uart::new` at 921600; **every printed byte lands here and is forgotten** | P6 |
| `IO_MUX` | `0x3FF4_9000` | `0x94` | direct #9, cycle 3,568,671 | the U0TXD pad | P8 |
| `SPI1` | `0x3FF4_2000` | `0x400` | direct #10, cycle 3,644,210 | esp-storage's flash read; **the run spins on `cmd.flash_rdsr`** | P7 |
| `SPI0` | `0x3FF4_3000` | `0x400` | direct #11, cycle 3,644,282 | the ROM's idle wait on `ext2.st` | P7 |
| `EFUSE` | `0x3FF5_A000` | `0x200` | rom-up #1, cycle 7 | the ROM's fuse read; **the ROM spins on `cmd.read_cmd`** | P5 |

Grades: every register is `documented` where the PAC calls it read-write
and the block pretends nothing, `modeled` otherwise (`RegFile::with_pac_
grades`); the analog master, which the PAC does not know, is `modeled`
throughout. Nothing is `measured`.

Not modelled, on purpose, and unmapped so a strict run says so: everything
the boot has not reached — `RTC_IO`, `SENS`, `RTC_I2C`, `FRC_TIMER`,
`FLASH_ENCRYPTION`, `SHA`, `RMT`, `RNG`, the WiFi window, and every AHB
mirror of a DPORT block.

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
| `src/periph/accept.rs` (unit) | every accept block reads the PAC's resets except the listed deviation, and every listed deviation is real and has a reason; the reset cause in both fields; the analog master's busy bit; no calibration pretence |

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
