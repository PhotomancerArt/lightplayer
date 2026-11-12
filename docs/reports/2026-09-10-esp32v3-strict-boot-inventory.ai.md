# The classic ESP32's strict bring-up pass — every stop, every pin, and the order P4–P7 take

**Plan:** `~/.photomancer/planning/lp2025/2026-09-10-0021-xtensa-emulator/`,
milestone M3 (`m3-esp32v3-machine-hello.md`), phase **P3**
(`m3/p3-strict-boot-discovery.md`). Branch `claude/xt-m3-p3-strict-boot`,
PR #679. Agent: fable, 2026-09-10.

**One-paragraph answer.** The shipped `fw-esp32v3` image, direct-loaded onto
the P2 machine under `--strict-bus`, needed **eleven** blocks to be named
before it stopped stopping; the mask ROM from its reset vector needed
**one**. Every one of the twelve is an accept-and-remember `RegFile` seeded
from the PAC's reset values, and exactly **one register** deviates from the
PAC (the reset cause, an input to the run). With those in place the direct
load prints its entire `[INIT]` chain — 543 bytes, the silicon capture's
lines through `[INIT] I/O task spawned` — into a register that remembers
only the last byte, and then spins on the flash controller's command word;
the ROM path spins on the eFuse controller's read command. Both spins are
registers only a model can answer, and both name their phase: **P7** and
**P5**. Along the way the pass found the classic's **second peripheral
window** (the AHB bus at `0x6000_0000`), reversed P1's exclusion of the
PAC's `RNG`, pinned the IDF bootloader's stack pointer at the app's entry
(`0x3FFE_3C80`), and recorded one divergence an accept block cannot fix
(the RTC calibration, which the firmware times out and answers with
26 MHz). No unsupported opcode was met on either path. No E-premise stop
was met.

## 1. The stop ledger

Every strict stop, in the order it was met, with the cycle it happened at,
the access, what answered it and the citation. "Answer" is the commit that
landed it; the branch's history is this table.

### 1.1 Entry A — the direct load

The image: `target/xtensa-esp32-none-elf/release-esp32v3/fw-esp32v3`, built
by `just build-fw-esp32v3` from this branch (`e_entry = 0x4008_0844`,
`Reset`). Every run: `--strict-bus`, `TimeGrade::T1` (cycles = instructions,
240 per µs), seed 0.

| # | cycle | pc | symbol | access | block · register | answer | citation |
|---|---:|---|---|---|---|---|---|
| A1 | 29 | `0x40125775` | `esp_hal::soc::xtensa::esp32_init+0x175` | W32 `0x3ff00218` | `DPORT` · `core_1_intr_map[0]` | accept, PAC resets, no exception | `esp-hal/src/soc/mod.rs:226` → `interrupt::setup_interrupts`; PAC `dport.rs` (49 non-zero resets) |
| A2 | 107,539 | `0x400081df` | `rtc_get_reset_reason+0xb` (mask ROM) | R32 `0x3ff48034` | `RTC_CNTL` · `reset_state` | accept; **deviation**: `reset_state = 0x3041` (POWERON in both cause fields) | ROM `400081e1: extui a2,a2,0,6` / `400081ed: extui a2,a2,6,6`; `esp-hal rtc_cntl/mod.rs:679-682`; L0 banner `rst:0x1 (POWERON_RESET)` |
| A3 | 109,663 | `0x400fce92` | `fw_esp32v3::boot_firmware+0x29a` (inlined `esp_hal::init`) | R32 `0x3ff66000` | `APB_CTRL` · `sysclk_conf` | accept, PAC resets | `esp-hal soc/esp32/clocks.rs:435-437` (`pre_div_cnt` modify), `:479-529` (tick confs) |
| A4 | 109,989 | `0x401268bb` | `esp_hal::clock::Clocks::measure_rtc_clock+0xb` | R32 `0x3ff5f068` | `TIMG0` · `rtccalicfg` | accept, PAC resets, **no pretence** — see §3.1 | `esp-hal clock/mod.rs:276-440`; `soc/esp32/clocks.rs:135-160` |
| A5 | 143,014 | `0x40004197` | `rom_chip_i2c_writeReg+0x2f` (mask ROM) | W32 `0x6000e010` — **outside every declared window** | `I2C_ANA_MST` · host 4 (BBPLL) | a **second MMIO window** (AHB) + accept with a hand table; no exception | ROM `40004177: l32r a9,(0x18003800)`; `4000418b: slli a9,a9,2`; spin `4000419e: bany a8,a10(1<<25)`; see §3.2 |
| A6 | 3,563,841 | `0x400fd669` | `boot_firmware+0xa71` | W32 `0x3ff60064` | `TIMG1` · `wdtwprotect` | accept (same table as TIMG0) | `esp_hal::init` → `Wdt::<TIMG1>::disable()`; PAC `timg0.rs` |
| A7 | 3,564,113 | `0x400fd9fa` | `boot_firmware+0xe02` | W32 `0x3ff44168` | `GPIO` · `func14_in_sel_cfg` (U0RXD) | accept, PAC resets (none) | `board/esp32v3/init.rs` `Uart::new(…).with_rx(GPIO3)` |
| A8 | 3,564,269 | `0x40126034` | `…clocks::UartInstance::configure_function_clock+0x98` | R32 `0x3ff40020` | `UART0` · `conf0` | accept, PAC resets | `esp-hal soc/esp32/clocks.rs:756-775` (`tick_ref_always_on`) |
| A9 | 3,568,671 | `0x400fe14b` | `boot_firmware+0x1553` | R32 `0x3ff49088` | `IO_MUX` · `gpio1` (U0TXD pad) | accept, PAC resets (none) | `init.rs` `.with_tx(GPIO1)` |
| A10 | 3,644,210 | `0x40083c98` | `esp_rom_spiflash_read+0xc` (the app's IRAM copy) | R32 `0x3ff42008` | `SPI1` · `ctrl` | accept, PAC resets | esp-storage's flash read for the `lpfs` mount; PAC `spi0.rs` (`user = 0x8000_0040`) |
| A11 | 3,644,282 | `0x400838aa` | `esp_rom_spiflash_wait_idle+0x1a` | R32 `0x3ff430f8` | `SPI0` · `ext2` | accept, PAC resets (`st = 0` is idle) | the ROM's idle wait polls both controllers |
| A-end | 72,000,000 (deadline) | `0x40083864` | `esp_rom_spiflash_read_status+0x38` | spin: `l32i; bnez` on `SPI1.cmd` | `SPI1` · `cmd.flash_rdsr` (bit 27) | **not answerable by an accept block** — P7 | `40083861: s32i a12,a10,0` (`1<<27`), `40083864..69: memw; l32i.n; bnez` |

Between A9 and A10 — cycles **3,622,541 to 3,642,925** — the boot wrote
543 bytes to `UART0.fifo`, decoded from a `--trace-block UART0` trace:

```text
[INIT] fw-esp32v3 boot
[INIT] chip=esp32 arch=xtensa heap=15072+112640+98304+15536=241552 (ROM PRO stack + dram_seg arena + SRAM1 tail + ROM APP stack)
[INIT] heap regions: 0 0x3ffe0440+15072 (ROM PRO stack), 1 .bss+112640 (dram_seg arena), 2 0x3ffe8000+98304 (SRAM1 tail), 3 0x3ffe4350+15536 (ROM APP stack, after core bind)
[INIT] main stack 45280 B
[RECOVERY] boot: cause=power-on level=green safe_mode=false prior_boot_complete=true
[INIT] runtime started
[INIT] I/O task spawned (uart0 921600 8N1, swi2 executor prio2, timg0t1 pacer 1ms)
```

The heap line is **byte-identical** to L0's silicon capture (`../bench.md`,
`heap=15072+112640+98304+15536=241552`); the main-stack figure differs
(45,280 vs 45,488) because the desk board runs a different, dirty commit
(`2e21b6226bcd-dirty`, ruling R7). The next silicon line, `[INIT] flash
filesystem mounted`, is the one after the flash read, and there is no flash
chip. **This is the "hello that came out of an accept block"** the phase file
predicted, and it is why P6 still models UART0.

### 1.2 Entry B — the mask ROM from its reset vector

| # | cycle | pc | symbol | access | block · register | answer | citation |
|---|---:|---|---|---|---|---|---|
| B1 | 7 | `0x4000fdd8` | `_ResetHandler_efuse_check_patch+0x38` (reported `~_rtc_trigger_sw_system_reset+0x11`) | R32 `0x3ff5a000` | `EFUSE` · `blk0_rdata0` | accept, PAC resets (all burned words 0) | ROM `4000fdd5..4000fdec`: reads `+0x00`, `+0x14`, `+0x18`, parks them at `0x3ffe1320` |
| B-end | 4,800,000 (deadline) | `0x4000fcae` | `_reload_efuses_and_check+0x1e` | spin: `l32i; bnez` on `EFUSE.cmd` | `EFUSE` · `cmd.read_cmd` (`+0x104`, bit 0) | **not answerable by an accept block** — P5 | `4000fc9b: conf = 0x5aa5`; `4000fc9d: cmd = 1`; `4000fca6: beqz → _rtc_trigger_sw_system_reset`; `4000fcac..ae: l32i; bnez` |

The ROM's anti-glitch check needs `cmd.read_cmd` to read **1** at least once
(a 0 sends it to a software reset) and then **0**. No constant satisfies
both, `RegFile::with_write_one_pulse` gives the wrong first read, and a
mirror has nothing to mirror: the read command is a *completion*, which is
P5's eFuse view. So the ROM path's value in P3 is the one stop above plus
the static scout in §4.3 — and the finding that **P5 gates P7**, which the
plan's order already has.

### 1.3 The stops that were not strict stops

Two places the run "got further" without a strict stop and where an accept
block's answer is *recorded* rather than *right*:

- **A4, the RTC calibration** (§3.1): the firmware's own timeout arm
  answers 0 after ~320 µs and again after ~6.8 ms; the run's cycle count
  jumps from 143,014 (A5) to 3,563,841 (A6) across those two loops.
- **A8, UART0**: every byte printed goes into `fifo`, which remembers the
  last one. `status.txfifo_cnt` reads 0, so neither the ROM's
  `uart_tx_one_char` (spins while `status & 0x0080_0000`) nor esp-hal's
  `write_bytes` ever waits.

### 1.4 Unsupported opcodes

**None** on either path. The direct load executed ~72 M instructions (to its
deadline, most of them the SPI1 spin) and the ROM path ~4.8 M (most of them
the eFuse spin) with `strict_unsupported` on, and neither raised the
unsupported-opcode stop. The `f64*`, MAC16, `lsi`, `witlb`, `rer`, boolean
and `loop` residue M0 measured therefore sits **off** the boot path as far
as P3 reached: nothing in `esp_hal::init`, the clock tree, the recovery
ledger, esp-rtos start, the I/O task spawn or esp-storage's first flash
read, and nothing in the ROM's reset handler through
`_reload_efuses_and_check`. The bootloader's `lsi` sites (M0: 314 of them)
and the ROM's `main` are beyond what P3 could execute (§4.3 says what P7
will run into first). DD16 holds: no `f64*` instruction was decoded, so
none was reported.

### 1.5 E-premise stops

**None.** Every spin met has a documented register behind it (the eFuse read
command's completion, the SPI command's completion, the calibration's
`rdy`), and the one undocumented block (the analog I2C master) spins on a
bit the ROM's own disassembly names and the guest never writes.

## 2. What the direct load pins — `loader.rs`

1. **`.data` is a self-copy**, verified on the image with
   `xtensa-esp32-elf-nm`: `_sidata = 0x3ffb0000`, `_data_start =
   0x3ffb0000`, `_data_end = 0x3ffb3054`. `hal-defaults.x:3-4` +
   `xtensa-lx-rt-0.22.0/src/lib.rs:225-227` make the app run its copy loop;
   `rwdata.x` with no `AT>` makes it a no-op. ⚠️ `m3/notes.md` §3 says the
   desk bootloader loaded `.data` at `vaddr=3ffb0010` after a 16-byte
   segment at `3ffb0000`; **this image** has one DRAM `PT_LOAD` at
   `0x3ffb0000` (`filesz 0x3054`) carrying `.data .bss .noinit .stack`. A
   different commit's layout, not a design change — the self-copy holds.
2. **The one relocated segment** is `.rtc_fast.persistent`: vaddr
   `0x3ff80000`, paddr `0x3f447410`, NOBITS. Placed by vaddr, recorded.
3. **The flash chip description's symbol is `spi_w25q16`, not
   `g_rom_flashchip`.** The vendored ROM ELF has no `g_rom_flashchip`; that
   is a name ESP-IDF's `esp32.rom.ld` `PROVIDE`s for `0x3ffae270`, which
   the ROM's own table labels `spi_w25q16` / `_data_start_spi_flash` — the
   32-byte `.data_spi_flash` section `rom::seed_data` places. Its
   `chip_size` word is `0x0020_0000` (2 MiB) and the loader writes 4 MiB
   over it. Notes §3 item 10 is corrected in the loader's docs and a test
   asserts the alias is absent.
4. **The bootloader's SP at the app's entry is `0x3FFE_3C80`** — `__stack`
   (`0x3FFE_3F20`) minus four `entry` frames: ROM `main` 112
   (`400076c4: entry a1,112`), IDF `call_start_cpu0` 192 (`4008064c: entry
   a1,192`), `bootloader_utility_load_boot_image` 304 (`40079a5c: entry
   a1,0x130`), `load_image` 64 (`400795a8: entry a1,64`); the app is entered
   by `callx8 a2` at `0x400796b9`, between the `Cache_Read_Enable` call and
   the next function's `entry`. The bootloader binary (espflash's embedded
   ESP-IDF `v5.1-beta1-378-gea5e0ff298`, raw-disassembled from the merged
   image's segments at `0x40078000` and `0x40080404`) contains no
   `movi`/`l32r`/`movsp` into `a1`: it runs on the ROM's stack.
   `BootFrame::idf_bootloader()` is the direct load's default;
   `BOOTLOADER_FRAME_CHAIN` carries the derivation and a test re-derives the
   constant. **What the four save-area words hold is P7's to measure** from
   the ROM-up run; `[0, sp, 0, 0]` stands until then.
5. `PS_BOOT = 0x0006_0020` (P2's, `CALLINC = 2`), not the phase file's
   `0x0004_0020`: the phase file predates P2's finding that the app is
   entered by `callx8`. Recorded as a phase-file correction, not a deviation.

## 3. Findings

### 3.1 The RTC calibration is the first thing an accept block cannot answer — and the firmware survives it wrong

`Clocks::measure_rtc_clock` (A4) sets `TIMG0.rtccalicfg.rtc_cali_start`
and polls `rtc_cali_rdy` (bit 15), then reads `rtccalicfg1.rtc_cali_value`.
The value is a **measurement** — XTAL cycles counted over `N` calibration
clock cycles — and the two callers want two different numbers:
`detect_xtal_freq` (10 cycles of RC_FAST/256) and
`calibrate_rtc_slow_clock` (1024 cycles of RC_SLOW). No constant serves
both, so the block carries **no override**: `rdy` never sets, and the
firmware's `#[cfg(esp32)]` timeout arm (`clock/mod.rs:433-440`,
`ets_delay_us(1)` per poll) answers 0 twice. Consequences on the direct
path today:

- `detect_xtal_freq` computes 0 MHz and picks **`XtalClkConfig::_26`**
  (`0.abs_diff(40) < 0.abs_diff(26)` is false). The desk board is 40 MHz.
  `RTC_CNTL.store4` and the BBPLL `regi2c` values reflect 26 MHz.
- `cal_val = 0` goes into `RTC_CNTL.store1`; anything reading the RTC slow
  clock period gets 0.
- The CPU still ends up at 240 MHz on the PLL (`cpu_per_conf` 0 → 2 at
  cycle 150,048) and APB at 80 MHz, so UART0's divisor and `ets_delay_us`
  are unaffected — which is why the `[INIT]` chain came out intact.

This is **P5's first job** (the TIMG view on `engine::timg` computes
`rtc_cali_value` from the clock tree), and until P5 lands the direct path's
clock state diverges from silicon in exactly the ways listed.

### 3.2 The classic has a second peripheral window, and the PAC's `RNG` was right all along

A5 wrote `0x6000_E010`, outside every declared window. The ROM's
`rom_chip_i2c_writeReg` computes `(0x1800_3800 + host_id) << 2`; the ROM's
`.text` loads forty-odd literals in `0x6000_0000..0x6002_2000` that line
up with PAC-named DPORT blocks offset by `0x3FF4_0000 − 0x6000_0000`
(`0x6000_88xx` = `SENS`, `0x6001_C0xx`/`0x6001_D0xx` = `NRX`/`BB`,
`0x6000_60xx` = `FLASH_ENCRYPTION`); and the PAC's `RNG` at `0x6003_5000`,
with `data` at `+0x144` = `0x6003_5144` — the classic's `WDEV_RND_REG` —
is WDEV seen from the same bus. **`AHB = 0x6000_0000 + (DPORT −
0x3FF4_0000)`**, `0x4_0000` long.

What landed: `memmap::MMIO_AHB_BASE/LEN` and `ahb_to_dport()`, a second
entry in `MMIO_WINDOWS` (`mmio-ahb`), `periph::I2C_ANA_MST` and
`periph::RNG`, the `I2C_ANA_MST` accept block with a hand-written name table
(the PAC has no block there) and every word graded *modeled*, the generator's
`rng` `SKIP` entry removed and `regs/rng.rs` generated, and the wrong claim
corrected in `memmap.rs`, `regs/mod.rs` and `tests/memmap.rs`. **P1's
exclusion was wrong** (`m3/notes.md` §4's "⚠️ The PAC's RNG base is wrong
for this chip" is the sentence to strike); the strict stop's `where` line
now names the window and the DPORT twin.

**What was not done, and is a ruling for the director:** the mirror is not
modelled. `SocBus` registers a peripheral at one base, and two registrations
would be two states — the SRAM1-alias mistake in MMIO form (DD24/DD36). So a
block lives at *one* of its addresses (the DPORT one where the PAC names it,
the AHB one where only the ROM does), and an access through the other is a
strict stop. The ROM's `main` and its helpers use the AHB addresses of
`SENS`, `FE`/`FE2`, `NRX`/`BB` and the analog master's other words (§4.3),
so **P7's ROM-up boot will hit the mirror**. Options: (a) a chip-side
forwarding view (`Arc<Mutex<RegFile>>` behind two `Peripheral` shims), (b)
an `add_peripheral_alias` on the shared bus (M2's file), (c) map the
ROM-used blocks at their AHB address only and let a DPORT access stop. The
C6 has no precedent; I recommend (b) as the honest one — one state, two
decodes — and it needs an M2 ruling before P7.

### 3.3 Two spins that name their phase, and the order they fix

- **`EFUSE.cmd.read_cmd`** must read 1 then 0 (B-end). P5.
- **`SPI1.cmd.flash_rdsr`** must clear when the status read completes
  (A-end); after it, `usr` for the read itself. P7.

Neither is expressible as a `RegFile` rule, and both are the first thing
their path meets after the accept blocks run out.

### 3.4 DPORT on the direct path — what P4 inherits (DD4)

From a `--trace-block DPORT` run to A-end: writes to `peri_clk_en` (`+0x1c`,
0), eleven read-modify-writes of `perip_clk_en` (`+0xc0`, from
`0xf9c1e06f` down to `0xf900600f` — `disable_peripherals` gating what the
image does not use), `cpu_per_conf` (`+0x3c`) 0 at cycle 109,828 then **2**
at 150,048 (XTAL → PLL, 240 MHz), and around UART0's construction
`perip_rst_en` (`+0xc4`) `0x0100_0000` then 0 and `perip_clk_en` again.
**No write to `pro_cache_ctrl` (`+0x40`), `pro_cache_ctrl1` (`+0x44`) or
`app_cache_ctrl` (`+0x58`)** before the flash spin: the direct path never
touched the cache-enable bits, so D4's stop has nothing to be defined
against yet on this path. The cache disable is inside the flash read that
the spin sits at the door of, so P7 is where it first appears; P4 should
build the stop from the ROM's `Cache_Read_Disable` (`0x4000_9AB8`) /
`Cache_Read_Enable` (`0x4000_9A84`) rather than wait for a trace.

### 3.5 Corrections to `m3/notes.md`

| where | says | found |
|---|---|---|
| §3 item 10 | the classic has `g_rom_flashchip` as a symbol | the ROM ELF names it `spi_w25q16`; `g_rom_flashchip` is ESP-IDF's linker alias |
| §3 | app segment 3 `.data` at `vaddr=3ffb0010` | this commit's image: one DRAM `PT_LOAD` at `0x3ffb0000`; `_data_start = 0x3ffb0000` |
| §4 | "the PAC's `RNG` base is wrong for this chip" | it is the AHB address of WDEV; the classic has two peripheral buses (§3.2) |
| phase file §1 | `PS = 0x0004_0020` | `PS_BOOT = 0x0006_0020` (P2's `callx8` finding) |
| P2 README | P3 pins the bootloader SP | `0x3FFE_3C80`, §2 item 4 |

## 4. The order and contents of P4–P7

The plan's order — **P3 → (P4 ∥ P5) → P6 → P7 → P8** — holds, with one
sharpening: **P5 must land the eFuse read-command completion and the TIMG
calibration before P7 can run ROM-up at all**, and P7 needs the AHB-mirror
ruling (§3.2) before its ROM-up boot reaches `main`. P4 and P5 stay
parallel. Below, per phase, what P3 leaves and what the boot needs first.

### P4 — DPORT, the interrupt matrix, the cache stop

- Replace `accept::dport()` with the view: `core_0_intr_map[src]` /
  `core_1_intr_map[src]` writes → `CpuIntMatrix::asserted(hart, irq)`;
  `cpu_intr_from_cpu[0..4]` (swi0 esp-rtos, swi2 io executor, swi1 the
  wire pusher); `appcpu_ctrl_{a,b,c,d}` with `appcpu_runstall` wired to
  `Machine::core_stalled(1)` alongside RTC_CNTL's two stall halves.
- `pro_cache_ctrl.pro_cache_enable` (bit 3) and the APP twin as per-core
  state, and D4's cache-off fetch stop with `--cache-off-fetch permit`;
  derive the flush/enable sequence from the ROM's `Cache_Read_Disable` /
  `Cache_Read_Enable` / `Cache_Flush` (§3.4: the direct path never wrote
  them before the flash spin).
- The flash MMU tables at `0x3FF1_0000` / `0x3FF1_2000` (raw 256-entry
  arrays; declare from the ROM's `cache_flash_mmu_set` at `0x4000_95E0`,
  which `mmu_init` and `main` call — §4.3). Not reached by P3.
- What the accept block already answers correctly and the view must keep:
  `perip_clk_en`/`perip_rst_en`/`peri_clk_en` remember-what-was-written,
  `cpu_per_conf` remembered (the value is read by the clock tree).

### P5 — RTC_CNTL, TIMG0/1, eFuse, and the accept list

- **eFuse first**: `cmd.read_cmd` as a completion (write 1 → busy for a
  bounded number of cycles → 0; the ROM checks *both* edges, §1.2), `conf`
  = `0x5aa5` as the read opcode, and `EfuseIdentity` seeding the burned
  words the ROM and esp-hal read — the desk board's MAC
  `30:76:f5:ec:f6:34` and silicon v3.1 (`blk0_rdata3`/`blk0_rdata5`
  revision bits). Until this lands the ROM path cannot leave
  `_reload_efuses_and_check`.
- **TIMG calibration**: `rtccalicfg`/`rtccalicfg1` on `engine::timg` with
  `rtc_cali_value` computed from the calibration clock (`RcFastDivClk` =
  8 MHz/256 for the XTAL estimate, `RcSlowClk` for the slow-clock period)
  so `detect_xtal_freq` answers 40 MHz and `store1` gets a real period
  (§3.1). Then the timers (`t0` esp-rtos tick, `t1` io pacer) and LACT.
- **RTC_CNTL**: `reset_state` stays an input (the one deviation, now the
  view's constructor argument); RWDT counting from `wdtconfig0..4` with
  the write-protect key `0x50D8_3AA1`; `options0`/`sw_cpu_stall` as the
  stall key's two halves; `clk_conf` (`dig_clk8m_d256_en`, `ck8m_*`),
  `store4` (the XTAL frequency esp-hal stashes for the ROM), `ana_conf`.
- **`I2C_ANA_MST`** as a `{block, register}` store keyed on the command
  word's `[7:0]` and `[15:8]` (the C6's `i2c_ana_mst` shape), keeping bit
  25 clear; today the image only writes through it (§1.1 A5).
- **The accept list**, unchanged blocks P5 keeps as `RegFile`s:
  `APB_CTRL` (A3). Not reached by P3 and still unmapped: `RTC_IO`, `SENS`,
  `RTC_I2C`, `FRC_TIMER`, `FLASH_ENCRYPTION` — the ROM's `main` reads
  `SENS` via its AHB address and `RTC_CNTL` `+0x88`/`+0xb0`/`+0xb8`/`+0xbc`
  (§4.3), so the ROM-up run in P7 will name them.

### P6 — UART0 and the CH340 cable

- `UART0` on `engine::uart` with the classic layout (`fifo` 0x00 … `id`
  0x7c): `status.txfifo_cnt` (bits 16:23) counting down at baud — the ROM's
  `uart_tx_one_char` tests **bit 23** of `status` (`0x0080_0000`) for
  "full", esp-hal's `write_bytes` reads the count; `clkdiv`/`conf0`/`conf1`
  as the image writes them (921600 8N1, `tick_ref_always_on`);
  `int_raw`/`int_ena`/`int_clr` for the io task's RX path; `mem_conf`.
- The 543 bytes in §1.1 are the acceptance: they must come out of the host
  stream, in that order, and `[INIT] I/O task spawned` is the last line
  before the flash.
- The CH340 cable on the control channel per `m3/notes.md` §6; the ROM's
  UART routines use the **DPORT** addresses (`(uart_no + 0x3ff4) << 16`),
  not the AHB mirror, so the mirror question does not gate P6.

### P7 — SPI0/SPI1, the flash chip, the cache, SHA, and ROM-up

- `SPI1` on `engine::spi_flash`: `cmd.flash_rdsr` (bit 27) completing to
  the status register, then `cmd.usr` (bit 18) for the read the mount
  wants; `user`/`user1`/`user2`, `addr`, `w0..w15`, `ctrl`, `ext2.st`; the
  chip the loader already sizes (`spi_w25q16.chip_size` = 4 MiB, `lpfs` at
  `0x0031_0000`). The direct path's first read is esp-storage's IRAM copy
  of `esp_rom_spiflash_read`, not the ROM's — the register sequence is the
  same one.
- `SPI0` as the cache's port + the cache MMU fill against the flash image
  (`stage_image_in_flash` in the loader, replacing item 2 of the eleven).
- **ROM-up prerequisites**, in the order the ROM meets them (§4.3): the
  AHB-mirror ruling (§3.2), P5's eFuse completion, then `main`'s
  `rtc_boot_control` (`RTC_CNTL +0xb8/+0xbc`), `uartAttach`/`Uart_Init`
  (`UART0 +0x10`, `UART1 +0x10`), `GPIO.strap` fifteen reads (the boot
  mode — `0x13` on the desk board), `IO_MUX +0x68/+0x88`,
  `spi_flash_attach` (`SPI1 +0x00/+0x08/+0x18/+0xfc`, `SPI0 +0x08..+0xfc`,
  `DPORT +0x18c/+0x190`), `mmu_init` (the MMU tables at `0x3FF1_0000` /
  `0x3FF1_2000`), `cache_flash_mmu_set`, `Cache_Read_Init` (`DPORT
  +0x40/+0x58`, `SPI0 +0x50`), `ets_unpack_flash_code` (`DPORT
  +0x44/+0x5c`, the MMU table, `GPIO.strap` again), then the bootloader's
  `SHA` for the image hash.
- The direct-load ↔ ROM-up cross-check: the eleven items in `loader.rs`,
  `BootFrame` (`a1 = 0x3FFE_3C80`, and the four save-area words P7
  measures), and the `.data` self-copy.

### 4.3 The ROM's own MMIO touches, statically (for P7)

Literals loaded by the routines `main` calls, from the vendored ELF's
disassembly — what the ROM-up boot will name, in roughly this order, once
the eFuse completion lets it past B-end:

| routine | DPORT-bus literals | AHB-bus literals |
|---|---|---|
| `main` | `DPORT +0x38/+0x44`, `GPIO +0x38` (`strap`, ×15), `RTC_CNTL +0x88/+0xb0`, `IO_MUX +0x68`, `EFUSE +0x18` | — |
| `rtc_boot_control` | `RTC_CNTL +0xb8/+0xbc` | — |
| `uartAttach` | `UART0 +0x10`, `UART1 +0x10` | — |
| `Uart_Init` | `DPORT +0x18c/+0x190`, `IO_MUX +0x88` | — |
| `spi_flash_attach` | `SPI1 +0x00/+0x08/+0x18/+0xfc`, `SPI0 +0x08/+0x18/+0x24/+0x28/+0x2c/+0x34/+0x50/+0xfc` | — |
| `mmu_init` | `0x3ff10000`, `0x3ff12000` (the flash MMU tables) | — |
| `Cache_Read_Init` | `DPORT +0x40/+0x58`, `SPI0 +0x50` | — |
| `ets_unpack_flash_code` | `DPORT +0x44/+0x5c`, `0x3ff10000`, `GPIO +0x38` | — |
| ROM `.text` overall | — | `0x6000_88xx` (`SENS`), `0x6000_50xx` (`FE2`), `0x6000_60xx` (`FLASH_ENCRYPTION`), `0x6000_e05x/e080` (the analog master's config words), `0x6001_c0xx`/`0x6001_d0xx` (`NRX`/`BB`), `0x6002_1000` |

## 5. How to reproduce

```bash
just build-fw-esp32v3
cargo run -p lp-emu-esp32v3 --release -- \
    --elf target/xtensa-esp32-none-elf/release-esp32v3/fw-esp32v3 \
    --strict-bus --timeout 300ms --trace target/xt-uart0.txt --trace-block UART0
#   DEADLINE cycle=72000000 (300000 us emulated) pc=0x40083864 (esp_rom_spiflash_read_status+0x38)
cargo run -p lp-emu-esp32v3 --release -- --boot-mode rom-up --strict-bus --timeout 20ms
#   DEADLINE cycle=4800000 (20000 us emulated) pc=0x4000fcae (~_reload_efuses_and_check+0x1e)
just test-emu-esp32v3-boot      # the ledger's endpoints and determinism, pinned
```

`Esp32V3Builder::bare()` reproduces P2's first stops; the P3 commits, one
per block, reproduce each row of §1.1 by checking out the parent of the
commit that answered it.
