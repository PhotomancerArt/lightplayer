# `lp-emu-esp32s3` — the ESP32-S3 machine

This is where the S3's chip numbers live. `lp-emu-esp-common` supplies the
bus, the peripheral model, the trace and the ELF view and knows nothing about
any chip; `lp-xt-emu` supplies the Xtensa hart and knows nothing about MMIO.
Here they are put together with a memory map, a mask ROM, a reset state and a
run loop, and the result takes a `fw-esp32s3` binary.

It is `lp-emu-esp32v3`'s twin — same hart, same shape, same module names — and
`lp-emu-esp32c6`'s where the peripherals are concerned.

> **M6 P05. The link works: the shipped image prints its `[INIT]` chain out
> of USB-Serial-JTAG, to a host a script or a socket can plug in and unplug.**
>
> **What exists:** the memory map ([`src/memmap.rs`](src/memmap.rs)), the bus
> it builds ([`src/bus_setup.rs`](src/bus_setup.rs)) **with SRAM1's I-bus view
> as a RAM alias**, the mask-ROM loader ([`src/rom.rs`](src/rom.rs)), the
> direct load ([`src/loader.rs`](src/loader.rs)), two hart slots and the
> quantum run loop ([`src/machine.rs`](src/machine.rs)), the snapshot, the
> generated register tables, a binary — `just emu-esp32s3 <elf>` — **every
> peripheral block the boot touches before it needs the console**
> ([`src/periph/`](src/periph/), [`src/intmatrix.rs`](src/intmatrix.rs); the
> table below) including **the RWDT that really runs**, and — from P05 — **the
> console and the host's side of its cable**
> ([`src/periph/usb_sj.rs`](src/periph/usb_sj.rs),
> [`src/control.rs`](src/control.rs)).
>
> **What does not exist:** the flash controller, the cache MMU, SHA and the
> ROM-up boot (P06) — which is where the boot now stops — and the pad fabric
> and RMT (P07).

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

P04's run stopped at the console — `0x6003_8004`, `USB_DEVICE.ep1_conf`, at
cycle 2,137,399, `esp_println`'s `jtag-serial` writer asking whether the IN
endpoint had room for the first byte of `[INIT]`. P05 models that block, so
**a strict run of the shipped image now refuses nothing**: it reaches its
deadline with `unmapped = 0`, and the whole `[INIT]` chain comes out of the
link.

```text
$ just emu-esp32s3 …/fw-esp32s3 --strict-bus --usb-host attached --timeout 2s
[INIT] fw-esp32s3 boot
[INIT] chip=esp32s3 arch=xtensa heap=245760
[RECOVERY] boot: cause=power-on level=green safe_mode=false prior_boot_complete=true
[RECOVERY] RWDT armed: boot 30000 ms, runtime 8000 ms
[INIT] runtime started
[INIT] I/O task spawned
usb-sj: host attached at power-on; 253 bytes reached the host; 0 bytes were merely tried
run: cycles=480000000 … unmapped=0 (reads 0, writes 0, 0 sites) …
```

⚠️ **Where it stops now is P06's, and it is a spin rather than a refusal.**
After `[INIT] I/O task spawned` the image mounts its filesystem, which is a
flash read: `esp_storage` sets `SPI1.cmd` bit 28 (`usr`) and spins until
hardware clears it. `SPI1` is an accept block, so the bit stays set — exactly
what [`periph::accept::spi1`](src/periph/accept.rs)'s own doc predicted in
P04. So the `lpfs` mount failure, the memory-filesystem fallback and
`[INIT] fw-esp32 initialized, starting server loop` are **P06's** readings,
not P05's, and `tests/boot_idle.rs` pins their absence with the register the
run is spinning on. The heap-ledger triple (`[stack]`, `[MEM]`, `[JIT]`) is
behind the same door: it is **elicited** by a request, and a request is
answered by the server loop.

⚠️ **The heap line is a single number**, `heap=245760` (`HEAP_SIZE = 240 *
1024`), where the classic prints a four-region sum. Anything comparing the two
chips' boot captures must not expect the same line. There is no
`[INIT] main stack <N> B` line either; the S3's 37,280 B total appears in
every `[stack]` line's `of <total> B`.

P03's stop — `SENSITIVE + 0x04` from the mask ROM's
`Cache_Occupy_ICache_MEMORY+0xc` at cycle 36 — is now the first line of the
trace, and `tests/boot.rs` asserts that line's pc is mask-ROM code, because on
this chip the ROM really executing is not a formality (§2.5: `memcpy` alone is
4,769 call sites).

## The peripherals, in the order the boot met them

`machine::PERIPHERAL_REGISTRATION_ORDER` is a contract — the bus packs a
block's index into every scheduler event id — and it is the ledger of P04's
strict loop: each block was added when the run stopped on it, with the cycle
of its first access on the shipped image. Grades are `lp-emu-validate`'s
ladder per register (`RegFile::with_pac_grades`): **documented** where the
PAC calls the register read-write and the block pretends nothing, **modeled**
otherwise; **nothing is `measured`** — no S3 silicon has been read yet.

| # | block | first access | what it is | source |
|---|---|---:|---|---|
| 1 | `SENSITIVE` | 36 | accept, PAC resets. The mask ROM's `Cache_Occupy_*_MEMORY` read-modify-writes `cache_dataarray_connect_1` and `internal_sram_usage_1` | fresh |
| 2 | `EXTMEM` | 71 | accept **with the operation-done bits answered**: every `*_sync_ctrl` / `*_preload_ctrl` / `*_lock_ctrl` operation bit is a pulse whose done bit reads 1, `cache_state` reads idle, and the two `*_freeze` done bits **mirror** their enable bits — the ROM waits for those both ways. The cache *model* is P06's | fresh; the C6's mirror idiom |
| 3 | `INTERRUPT_CORE1` | 403 | a register view over the bus's matrix — core 1's half. esp-hal clears the *other* core's map first | the classic's `intmatrix.rs` |
| 4 | `INTERRUPT_CORE0` | 407 | core 0's half of the same 4 KB block | the classic's |
| 5 | `RTC_CNTL` | 205,054 | the reset cause (asserted `POWERON_RESET`, the one PAC deviation), the two-register stall key, **the RWDT as a real scheduler counter** with its key, the super-watchdog with its own key `0x8F1D_312A`, the clock and store registers | the classic's `rtc_cntl.rs`, offset table `+4` |
| 6 | `SYSTEM` | 206,309 | the clock and reset gates written-and-read-back; `core_1_control_0` through the machine's hold handle; `cpu_intr_from_cpu[0..4]` as sources 79..82 | fresh |
| 7 | `EFUSE` | 207,842 | the MAC and the wafer version, **in the S3's own words** (minor split across `+0x50`/`+0x58`, major in `+0x58` — not the C6's `+0x50`); PAC zero everywhere else, no dump seeded (P09) | the C6's `efuse.rs` |
| 8 | `I2C_ANA_MST` | 207,952 | the analog master as a `{block, register}` store behind the ROM's eight command words, plus the PAC's three registers with `ana_conf0.bbpll_cal_done` reading 1 (esp-hal spins on it) | the classic's `i2c_ana_mst.rs` |
| 9 | `TIMG0` | 288,925 | **two** counters on `engine::timg` (XTAL or APB per `use_xtal`), the RTC calibration `calibrate_rtc_slow_clock` really runs (1024 cycles of 136 kHz = the 7.5 ms gap to the next row), the MWDT gate; **no LACT**, asserted | the C6's `timg.rs`, counter count 1 → 2 |
| 10 | `APB_CTRL` | 2,097,791 | accept, PAC resets (`front_end_mem_pd`, `clkgate_force_on`, `mem_power_up`) | fresh |
| 11 | `SPI0` | 2,097,806 | accept, PAC resets (`clock_gate`); the flash port is P06's | fresh |
| 12 | `SPI1` | 2,097,812 | accept, PAC resets (`clock_gate`); the flash controller is P06's | fresh |
| 13 | `BB` | 2,097,896 | accept, one register (`bbpd_ctrl`) | fresh |
| 14 | `NRX` | 2,097,903 | accept, one register (`nrxpd_ctrl`) | fresh |
| 15 | `FE` | 2,097,910 | accept, one register (`gen_ctrl`) | fresh |
| 16 | `FE2` | 2,097,917 | accept, one register (`tx_interp_ctrl`) | fresh |
| 17 | `TIMG1` | 2,098,065 | the same view; met only for its watchdog disable | the C6's |
| 18 | `SYSTIMER` | — | **the S3's `Instant::now()`**: Unit0 at XTAL/2.5 = 16 MHz, one tick per 15 cycles, `micros = ticks >> 4`; three comparators. ⚠️ Not reached before the console — `time_init` on this chip touches no register — but registered, and `tests/clock.rs` pins the derivation | the C6's `systimer.rs`, verbatim |
| 19 | `USB_DEVICE` | 2,137,399 | **the link, and the console on it** — the host's three states and the transitions between them, source 96. ⚠️ **Appended, not inserted**: the boot meets it *before* `SYSTIMER`, but re-sorting the pair would move `SYSTIMER`'s index and the bus packs that index into every scheduler event id | the C6's view, **moved** to `lp-emu-esp-common/src/ip/usb_sj.rs` and parameterised |

Which parent each block came from matters, and the wrong one is silently
wrong (`m6/notes.md` §3): `RTC_CNTL`, `I2C_ANA_MST` and the matrix are the
**classic's**; `TIMG`, `SYSTIMER` and `EFUSE` are the **C6's**. The copies
live in this crate because the plan's invariant is that the C6 and the
classic do not move by a byte; the extraction into `lp-emu-esp-common` is
M8's, and each file names what it would extract.

### Two things the accept blocks needed that a PAC reset does not give

- **`EXTMEM`'s freeze pair.** The ROM's `Cache_Freeze_*_Enable` sets bit 0
  and spins until `done` (bit 2) is **1**; `Cache_Freeze_*_Disable` clears
  bit 0 and spins until it is **0**. The first traced run stood at
  `Cache_Freeze_DCache_Disable+0x1b` for two emulated seconds on the PAC's
  reset `0x04`. No constant satisfies both polls; a read **mirror** does, and
  says the true thing. `periph::accept::READ_MIRRORS` lists both with the
  disassembly beside them; `READ_OVERRIDES` lists every other pretended bit.
- **`I2C_ANA_MST.ana_conf0.bbpll_cal_done`.** esp-hal's `enable_pll_clk_impl`
  spins `while … bbpll_cal_done().bit_is_clear() {}` (`clocks.rs:232-238`)
  and the PAC gives the register no reset. It reads 1, *modeled*, with the
  citation.

### The watchdog that really runs

The shipped firmware arms the RWDT at 30 s, tightens it to 8 s on the first
feed, and withholds the feed when the io task has been silent for 2 s — there
is no `disable()` anywhere in the crate (`m6/notes.md` §5.2). So stage 0 is a
real counter here: `hold × 2 / RC_SLOW_HZ` seconds after the last feed or arm
(esp-hal's `set_timeout` shifts right by `1 + WDT_DELAY_SEL`, which reads 0),
and expiry asks the machine for a reset, which the run reports as
`Outcome::Reset` (exit 2) because there is no boot chain to restart until P06.
`tests/clock.rs` runs both directions through the machine at the real
quantum: armed for 30 s and never fed, the run ends in a reset at cycle
7,200,000,010; armed for 8 s and fed once a second by a `CCOMPARE1` handler,
forty seconds pass with no reset, and 7 s after the feeds stop it bites.

### X43, on this chip

The matrix answers the **mask** form and never implements `cpu_interrupt`
(`intmatrix.rs`'s module docs). `tests/clock.rs` raises `FROM_CPU_INTR0`
through `SYSTEM.cpu_intr_from_cpu0` with `INTENABLE` still 0, performs one
more MMIO store — the store that zeroed the mask under X43 — then enables the
line, and asserts the take followed within a few instructions (CCOUNT 12 →
20) rather than at the next 256-cycle window boundary. Never single-stepped.

## The link, and what a host can and cannot do to this chip through it

The console **is** the link: `esp-println` with the `jtag-serial` feature
writes `ep1` by raw MMIO, and there is no `spike_uart0_link` build of this
firmware — its own module says so, *"The S3 has no `spike_uart0_link` build,
so its link is always the real USB one and SOF always gates writes"*. One
image, one link, no cherry-pick. `--console <path>` therefore writes the
`usb-sj` stream.

The model is the C6's, **moved** rather than copied (ruling D1 (b) / DD64):
`lp-emu-esp-common/src/ip/usb_sj.rs` holds the view and this crate's
[`src/periph/usb_sj.rs`](src/periph/usb_sj.rs) holds the S3's parameters. The
host has three states and the transitions between them are the product:

| state | `--usb-host` | `int_raw.sof` | what the guest sees |
|---|---|---|---|
| no cable | `absent` | never | the first packet commits and `free` never comes back; esp-println latches `TIMED_OUT` and the console falls silent |
| cable in, port closed | `attached-idle` | every 1 ms | the packet is **held**, not dropped; an `open` delivers it |
| cable in, application draining | `attached` | every 1 ms | packets cross after 100 us (*modeled*) |

Two sockets drive it, and they are different things. `--usb-sj tcp:<addr>`
carries **bytes** — `lp-cli … serial:tcp://` connects to it unchanged — and a
client connecting **is** an application opening the port (`--usb-sj-drain
manual` decouples them). `--control tcp:<addr>` carries the **cable**, one
line per command and one reply per command: `attach`, `detach`, `open`,
`close`, `dtr`, `rts`, `signals`, `reset`, `download-mode`, `state`,
`usb-write`. `attach` and `detach` are never implied by a socket: a cable is
not a port open, and the whole reason to model a host is that the two come
apart. `--usb-script <file>` is the deterministic twin — declared **emulated**
times, plus `after "<line>"` and `then +<ms>` — and two runs of one script
deliver identical bytes at identical cycles; a socket is host time and is
only auditable.

### ⚠️ Three things a reader who knows the C6 will look for and not find

1. **No `chip_rst`, so a serial-channel reset is unconditional and the guest
   has no say.** `0x4c`…`0x7c` is reserved on this part
   (`esp32s3-0.35.2/src/usb_device.rs:22`): there is no
   `disable_usb_serial_chip_reset` bit for a guest to set, so `reset` and
   `download-mode` always take effect, and the C6's `err` reply naming that
   bit has no counterpart here. (`bus_reset_st` is missing for the same
   reason, so a bus reset releases nothing.) With `--reboot-on-reset` the
   machine goes back to its power-on state and runs again; without it the run
   ends and names who asked, exit code 2.
2. **The shipped image never reads DTR or RTS.**
   `UsbConnectionMonitor::poll` reads `int_raw.sof` and clears it and does
   nothing else, and `int_raw` bits 12–15 are not declared on this part at
   all. The `dtr` / `rts` / `signals` verbs stay because a host really does
   assert those lines — esptool's dances are made of them, and P06's flasher
   path needs them — they simply reach no guest-visible register.
3. **No `pin` / `pins`, and no CH340 truth table.** The pad verbs are the
   fabric's and the fabric is P07's, so the parser refuses them **by name**
   with the phase that owns them rather than as a typo. And the classic's
   auto-reset truth table is about a wire between a USB-serial bridge and two
   pins of the chip; this link *is* the chip, which is why an S3 can be
   flashed over Web Serial at all, and why **a reset does not re-enumerate
   the port** — only `detach` then `attach` mints a new one.

### Grades: every register of this block is `modeled`

⚠️ **A transcript recorded on a C6 is a measurement of a C6.** The six
`measured` grades the C6's block carries were bought by four committed
transcripts under `lp-emu/transcripts/esp32c6/`, replayed against silicon
captures of *that* chip. No S3 silicon has been read (P09 owns that), so this
chip's grade table is **empty** and every register answers `modeled` — the
data path included. A run under `--strict-grade documented` stops at the first
register the console touches, which is correct and is what the flag is for.

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

Twenty-three blocks: every one the image's own MMIO census names, plus `uart0`
and `sha` for the ROM-up path, plus `sensitive` — the block P03's first
strict stop found, reached from the **mask ROM** and invisible to a census of
the application's literals (P04 added it to the generator's list). A table
nothing reads is cheap; a missing one is a phase blocked on a regenerate.

**The interrupt-source table is generated too** (`src/regs/interrupt_sources.rs`,
the `source` module). `esp32s3-0.35.2` keeps its `Interrupt` enum in
`src/lib.rs` rather than in a `src/interrupt.rs` as the C6's PAC does; the
generator's `sources_file` field (P04) is the one-line difference. Ninety-four
named sources over the number range `0..=98`, with gaps — which is why the
matrix has 99 map entries and the table 94 rows.

## Licence

MIT, as a unit with the rest of `lp-emu/` — not the workspace's AGPL. See
`lp-emu/LICENSE-MIT`, `docs/adr/2026-09-06-lp-emu-home-and-mit-fence.md` and
`just lint-emu-fence`.
