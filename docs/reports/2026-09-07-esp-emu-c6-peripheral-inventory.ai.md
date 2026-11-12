# esp-emu / ESP32-C6 peripheral & register inventory

2026-09-07

## Provenance

Two different `fw-esp32c6` images feed this report, and the distinction matters for reading
Table B: the **static** columns come from `objdump`-scanning the **default (shipped)** firmware
ELF — no spike feature, USB-Serial-JTAG console — built from this worktree at base commit
`d6cfaa205` (`fw-default.nm`/`.dis`, scanned by `mmio_scan.py` into `mmio-default.json`, 748
resolved static MMIO sites). The **dynamic** columns come from Espressif's binary emulator,
`esp-emu` v0.42.0 (`--chip esp32c6`), running a second image built on the same base with the
spike feature `spike_uart0_link` enabled (`lp-fw/fw-esp32c6/src/serial/spike_uart0.rs`), which
swaps the host link from USB-Serial-JTAG to a raw UART0 `Uart` driver. Two traces were captured
under `RUST_LOG=trace`:

- **traceE** (`walks/traceE.emu.stderr`, 2.70 MB / 50,484 lines): full walk — boot, `hello`,
  `lp-cli upload examples/basic`, project load, shader JIT compile, then ~60 s of idle rendering
  at the emulator's free-running rate (the CLI log shows the "Basic" project's shader compiling
  in 52 ms, `lpir_inst_count=573`, `final_code_size=8192` bytes, then RMT WS281x output opened on
  GPIO18).
- **run2-default-trace** (`run2-default-trace.stderr`, 422 KB / 5,987 lines): 8 s boot-only trace
  of the plain **default** image (no spike feature — the one the static scan covers), used only
  to sanity-check that the boot-time peripheral touches are the same regardless of which host
  link is compiled in.

Because the spike image is UART0-only for its host link, every UART0 register named in Table B
comes from the dynamic trace, not the static scan (the static scan's 3 UART0 "hits" are a single
site in a slot-path error-formatting function and are very likely a false positive of the
scanner's basic-block-scoped register tracking, not a real UART0 access in the default image).

PAC register names come from `esp32c6-0.23.2`'s `src/*.rs` (svd2rust `#[doc = "0xNN - ..."]`
offset comments); base addresses from `c6-pac-bases.txt`. Where the static scanner's
nearest-base heuristic (everything between two known bases gets attributed to the lower one)
reaches into a wide gap with no real register file that large, it pulls in addresses that almost
certainly belong to the undocumented WiFi PHY/MAC/baseband blob instead of the named block; those
sites are called out under Table A rather than counted against the named PAC block.

## Table A — Peripheral blocks the firmware touches

| Block | Base | Evidence | Distinct regs | Access counts | esp-emu status | Firmware role |
|---|---|---|---:|---|---|---|
| UART0 | 0x6000_0000 | dynamic only (spike image) | 7 offsets | dynamic: 1,692 trace lines (804 R + 2 W @0x064 `tout_conf`, 802 R @0x05C `at_cmd_char`, 50 R + 25 W @0x098 `reg_update`, a few one-off R/W @0x02C/0x03C/0x048/0x088) | modeled + logged (`periph::uart`) | `spike_uart0_link`'s `esp_hal::uart::Uart` async driver (host link over UART0, GPIO16 TX/GPIO17 RX); the two heavy offsets are polled continuously through the 60 s idle-render tail |
| USB_DEVICE (USB-Serial-JTAG) | 0x6000_f000 | static only (default image) | 6 offsets | static: 14 sites | modeled-silent (no `periph::usb_device`/`usb_serial_jtag` trace tag exists; also **never produces a chip-unhandled hit**, so it is a real, silent model) | `esp_println` console TX/RX (`0x000` FIFO, `0x004` EP1_CONF), `usb_connection` liveness monitor and esp-hal's async interrupt handler (`0x00C`/`0x010`/`0x014` INT_ST/INT_ENA/INT_CLR); the `0x018` CONF0 sites attributed to the button and WS281x `open()` paths are static-scan attribution (an inlined flush in a logging path). **Proven live through the gdb stub (main report §4): the model asserts SOF permanently (`INT_RAW = 0xA`, `INT_CLR` ignored) and reports EP1 "data free" forever, so the firmware believes a draining host is attached and every byte written vanishes — this is why the spike's UART0 link exists** |
| RMT | 0x6000_6000 | static only | 5 offsets | static: 9 sites | modeled-silent | `fw_esp32c6::output::rmt` WS281x driver (`shared_driver::rmt_isr`, `adopt_channel`, channel write) |
| TIMG0 | 0x6000_8000 | static only | 3 offsets | static: 12 sites | modeled-silent | `esp_hal::init`, TIMG calibration clock config, `Wdt<TIMG0>::new` |
| TIMG1 | 0x6000_9000 | static only | 1 offset | static: 4 sites | modeled-silent | `esp_hal::init` |
| SYSTIMER | 0x6000_a000 | static only | 5 offsets | static: 64 sites | modeled-silent | `esp_hal::time::Instant::now`, `esp_rtos::timer::TimeDriver::arm_next_wakeup`, and heavily by `esp_radio::common_adapter` (semaphore/queue timeouts) |
| SPI0 (flash cache/MMU) | 0x6000_2000 | dynamic only, tagged `spimem` (shared with SPI1, see notes) | ≥3 offsets (`ctrl`, `cache_fctrl`, MMU table) | dynamic: `cache_fctrl` (0x03C) 3,034 R + 3,034 W; `ctrl` (0x008) 3,080 R + 141 W; one-off hits at 0x00C/0x010/0x014 | modeled + logged (`periph::spimem`) for named offsets it recognizes, else falls through to `SpiMem *unhandled offset*` (same tag, different message) | Mask-ROM flash driver's cache/MMU control path (paired with the one-shot `esp32c6::extmem` "MMU entry[n]" dump, 3,235 lines, at boot) |
| SPI1 (flash command host) | 0x6000_3000 | static (3 sites, `boot_firmware`) + dynamic (`spimem` tag) | ctrl2/clock plus command decode | dynamic: `SPI USR cmd` 14,169 lines, `USR READ` 13,528, `FLASH_RDSR` 5,103, `FLASH_WREN` 664, `USR PP` 634, `FLASH_SE` 29, `FLASH_RDID` 2, `FLASH_WRDI` 1 | modeled + logged (`periph::spimem`, `DEBUG` level for decoded commands) | Mask-ROM SPI flash driver (`esp_image` bootloader + littlefs writes for the `lpfs` partition during `lp-cli upload`) — this is the single busiest peripheral in the whole walk |
| EXTMEM | 0x600c_8000 | dynamic only, one-shot | n/a (state dump, not per-offset MMIO) | dynamic: 3,235 `DEBUG` lines, identical count in the 8 s boot-only trace and the 120 s full walk | modeled + logged, but as an **internal cache/MMU state dump**, not real bus traffic — see notes | Cache/MMU page-table population during ROM cache init (this is really SPI0's `mmu_item_content`/`mmu_item_index` array, offset 0x37C+, being narrated under the `esp32c6::extmem` tag rather than `periph::spimem`) |
| LP_WDT | 0x600b_1c00 | static (6 real offsets, 21 sites) + dynamic (1 unhandled offset) | 6 real + 1 phantom | static: `wdtconfig0`/`wdtconfig1`/`wdtfeed`/`wdtwprotect`/`swd_conf`/`swd_wprotect` all modeled-silent, 0 trace lines; dynamic: offset 0x054 (undocumented reserved padding) hit 18 times (9 R + 9 W), always via the chip-generic unhandled path | split: real registers modeled-silent; offset 0x054 unhandled | `esp_hal::init`, `boot_firmware`, `Rwdt::set_timeout` (boot-time watchdog arm/feed). The 0x054 hits are unexplained — no static site resolves to it; possibly an out-of-range `wdtconfig(n)` index or a genuinely undocumented register |
| LP_AON | 0x600b_1000 | static (5 offsets, 37 sites) + dynamic (`LP_AON` tag, 3 offsets) | 5 | dynamic: `gpio_hold0` (0x02C) 8R+8W, `sar_cct` (0x054) 1R+1W, `ext_wakeup_cntl` (0x040) 1R | modeled + logged (`periph::rtc_cntl`, message prefix "LP_AON") | `esp_hal::init`, `pwdet_reg_init_new`, `Rwdt::set_timeout` — GPIO pad-hold and wake-source bookkeeping across the RTC domain |
| PMU | 0x600b_0000 | static (60 offsets, 143 sites) + dynamic (tagged `rtc_cntl`, see notes) | 60 | dynamic: ~50 distinct offset hits below 0x0400, mostly 1-12 each (heaviest: `rf_pwc` @0x154, 12R+10W) | modeled + logged, but under the legacy `periph::rtc_cntl` tag (see notes) | `esp_hal::init` power-domain sequencing (`hp_active_*`/`hp_sleep_*`/`imm_*` registers), `open_i2c_xpd_new` |
| LP_CLKRST | 0x600b_0400 | static (3 offsets, 13 sites) + dynamic (same `rtc_cntl` tag, offsets ≥0x400) | 3 | dynamic: `lp_clk_conf`(+0x400) 7R+4W, `fosc_cntl`(+0x418) 2R+2W, `rc32k_cntl`(+0x41C) 2R+2W, `clk_to_hp`(+0x420) 5R+5W | modeled + logged, under the `rtc_cntl` tag | `esp_hal::soc::implementation::clocks` (RC-fast/XTAL32K clock request/release paths) |
| EFUSE | 0x600b_0800 | static (7 offsets, 15 sites: `rd_repeat_data1`+0x34, `rd_repeat_data2`+0x38, `rd_mac_spi_sys_0..4`+0x44/0x48/0x4C/0x50/0x54) + dynamic (5 offsets, see notes) | 7 static + 5 dynamic | dynamic: offsets 0x410/0x414/0x418/0x440/0x444 (8R+8W / 8R / 8R / 1R+1W / 1R+1W) — **these exceed the PAC's documented efuse register file (max offset 0x1FC, `date`)**, so they cannot be named from the PAC; either the trace's own offset numbering isn't relative to 0x600b_0800, or esp-emu's efuse model uses a larger legacy layout shared across chip variants | modeled + logged (`periph::efuse`) for the offsets it recognizes, unnamed-but-logged for the rest | `esp_hal::efuse::implem::{major,minor}_chip_version`, `esp_phy_efuse_get_mac`, `boot_firmware` |
| MODEM_SYSCON | 0x600a_9800 | static (28 sites at 3 in-range offsets; **156 more sites at addresses 0x600ad000-0x600ad0b4 are almost certainly WiFi PHY/BB blob, not this block** — see notes) + dynamic (3 offsets, all unhandled) | 3 genuine | dynamic: `clk_conf_power_st`(+0x00C) 4, `clk_conf1`(+0x014) 12, `wifi_bb_cfg`(+0x01C) 12 — **100% unhandled**, i.e. reads return 0 and writes are dropped | **unhandled** (no `periph::modem_syscon` tag exists in esp-emu at all) | `boot_firmware`, `hal_enable_sta_tbtt` (WiFi/BT modem clock gating — entirely unmodeled) |
| MODEM_LPCON | 0x600a_f000 | static (5 offsets, 31 sites) + dynamic (5 offsets) | 5 | dynamic: `wifi_lp_clk_conf`(+0x00C) 8, `i2c_mst_clk_conf`(+0x010) 54, `modem_32k_clk_conf`(+0x014) 10, `clk_conf`(+0x018) 110, `clk_conf_power_st`(+0x020) 10 — **100% unhandled** | **unhandled** | `boot_firmware`, `regi2c_enable_block`, `esp_radio::wifi::os_adapter::{phy_disable,phy_enable}` — low-power modem clock gating, entirely unmodeled |
| LP_I2C_ANA_MST | 0x600b_2400 | static: none in default image | 4 | dynamic: `i2c0_ctrl`(+0x000) 43, `i2c0_data`(+0x008) 20, `device_en`(+0x014) 46, `date`(+0x3FC) 23 — **100% unhandled** | **unhandled** | Analog-domain I2C master used by PHY/RF calibration at the LP boundary; entirely unmodeled |
| I2C_ANA_MST | 0x600a_f800 | static (36 offsets, 56 sites) | 36 | not separately observed in the dynamic trace (falls inside the wide `IEEE802154`/PHY blob region esp-emu treats generically) | modeled-silent (no `periph::i2c_ana_mst` tag; no unhandled hits recorded at these addresses either) | `phy_i2c_master_cmd_mem_init`, `phy_i2c_init2`, `i2c_clk_sel`, `enable_pll_clk_impl` — PHY calibration's own I2C-style register bus |
| ASSIST_DEBUG | 0x600c_2000 | static (1 offset, 2 sites — undercounts, see notes) + dynamic (3 offsets) | 3 real | dynamic: `cpu(0).rcd_en`(+0x044) 8, `cpu(0).rcd_pdebugpc`(+0x048) 3, `cpu(0).debug_mode`(+0x074) 22 — **100% unhandled** | **unhandled** | `boot_firmware`, `esp_rtos::task::idle_hook` — this is ESP32-C6's hardware stack-guard/debug-monitor block, a plausible **hardware alternative to the RISC-V debug-trigger CSRs** used for the same stack-guard purpose (see Table D) |
| IO_MUX | 0x6009_0000 | static: none in default image | 14 unhandled + presumably ~17 modeled-silent (GPIO0-15 pad configs, never separately confirmed since no static/dynamic hit landed there) | dynamic: `gpio(16..30)` pad-config registers (offsets 0x044-0x07C), each 1-5 R/W, **all unhandled**; `gpio(18)` (GPIO18, the RMT WS281x output pin) is among them | **split**: pads for GPIO0-15 appear to be modeled (never surface as unhandled), pads for **GPIO16-30 are entirely unhandled** — the model's IO_MUX table stops at pin 15 | esp-hal's whole-chip pad-init walk during `esp_hal::init` touching every GPIO's `IO_MUX_GPIOn_REG`; GPIO18's pad-mux write (routing RMT signal 0 onto GPIO18 for the WS281x driver) lands in the unhandled range |
| GPIO | 0x6009_1000 | static (3 offsets, 3 sites) | 3 | none observed as chip-unhandled | modeled-silent | Button driver (`Esp32GpioButtonDriver::open`, `Esp32ButtonInput::poll`) and the default GPIO interrupt handler |
| PCR | 0x6009_6000 | static (37 offsets, 93 sites) + dynamic (tagged `system_reg`, 60+ offsets) | 37+ | dynamic: heaviest are `sysclk_conf`(0x110) 11R+3W, `mspi_clk_conf`(0x01C) 7R+7W, `cache_conf`(0x104) 6R+6W; full clock-gate sweep across nearly every named PCR register (UART, RMT, LEDC, TIMG×2, SYSTIMER, TWAI×2, I2S, SARADC, USB_DEVICE, GDMA, AES/SHA/RSA/ECC/DS/HMAC, IO_MUX, cache, CPU/AHB/APB clocks) | modeled + logged, but under the legacy `periph::system_reg` tag (see notes) | `PeripheralClockControl::enable_forced_with_counts`, `esp_hal::init`'s clock-gate bring-up for essentially every peripheral in the image, including ones with zero MMIO evidence of their own (**GDMA's clock gate at PCR+0x0BC is the only trace of GDMA in this whole walk — no GDMA register itself is ever touched**) |
| INTPRI | 0x600c_5000 | static (5 offsets, 13 sites) | 5 | none observed as chip-unhandled | modeled-silent | `esp_radio` interrupt on/off, `SoftwareInterrupt<0>::raise`, `esp_rtos` embassy waker `__pender` (software-interrupt doorbell) |
| PLIC_MX | 0x2000_1000 | static (4 offsets, 9 sites) | 4 | none observed as chip-unhandled | modeled-silent | `boot_firmware`, `_setup_interrupts`, `handle_interrupts`, `change_current_runlevel` — the RISC-V platform interrupt controller |
| INTERRUPT_CORE0 | 0x6001_0000 | static (6 offsets, 9 sites) | 6 | none observed as chip-unhandled | modeled-silent | `esp_radio` ISR registration, `boot_firmware`, `esp_hal::init` |
| WIFI MAC/BB (undocumented; PAC calls this window `IEEE802154`) | 0x600a_0000-0x600a_97ff (+ the misattributed MODEM_SYSCON/LP_APM0 spillover noted above) | static (169 offsets, 762 sites directly, +156 via MODEM_SYSCON spillover, +170 via LP_APM0 spillover ≈ 1,088 sites total) + dynamic (`periph::wifi_mac`, 7 lines) | 169+ | dynamic: 4× "WiFi INT clear", 3× "WiFi RX config" (`dma_base=0x15DBC`, enabled true/false) | modeled + logged for the handful of high-level operations esp-emu recognizes (`periph::wifi_mac`); the bulk of raw register traffic below it is invisible (falls inside the same generic window, never surfaces as chip-unhandled either) | `esp_radio`'s WiFi/BT baseband and MAC blob: `mac_txrx_init`, `hal_he_clr_multi_bssid`, `hal_init_imrsp_power`, `rfcal_rxiq_new`/`rfcal_txiq_new` (RF calibration) |
| LP_APM0 | 0x6009_9800 | static: 1 genuine offset (`func_ctrl` @ +0x0C4, 1 site) — the other 45 "LP_APM0" addresses the scanner found (170 sites) are folded into the WIFI MAC/BB row above | 1 | none | modeled-silent (no dynamic hit either way) | `esp_hal::init` |
| RNG | 0x600b_2800 (PAC also lists `LP_PERI` at the identical base) | static (1 offset, 1 site) | 1 | none | modeled-silent | `esp_hal::rng::ll::fill_ptr_range` |
| HP_APM | 0x6009_9000 | static (1 offset, 1 site) | 1 | none | modeled-silent | `esp_hal::init` |
| LP_APM | 0x600b_3800 | static (1 offset, 1 site) | 1 | none | modeled-silent | `esp_hal::init` |
| GDMA | 0x6008_0000 | **no static sites, no dynamic sites, at all** | 0 | 0 | unknown/unexercised | Its only footprint in the whole inventory is the PCR clock-gate bit (`gdma_conf` @ PCR+0x0BC); the spike image's WS281x/button/serial paths never touch a DMA-capable peripheral, so GDMA itself is unexercised in this walk, not necessarily unimplemented |

Notes on the table above:

- **The "RTC_CNTL" trace tag is a legacy-shaped abstraction leak.** esp-emu logs offsets
  0x000-0x1A0 under `periph::rtc_cntl` as if there were one classic ESP32-style `RTC_CNTL` block
  at 0x600b_0000; every offset in that range maps cleanly onto a real **PMU** register name, and
  every offset ≥0x400 maps cleanly onto **LP_CLKRST** (PMU's real aperture ends at 0x600b_0400,
  where LP_CLKRST begins) — offset arithmetic, not the emulator's own labeling, is what recovers
  the real PMU/LP_CLKRST split.
- **The "SYSTEM" trace tag (`periph::system_reg`) is the same kind of leak for PCR.** Every
  `SYSTEM read/write offset` seen in the trace matches a real PCR register name 1:1, including
  the two-element `uart[]`/`timergroup[]` clusters at the low offsets.
- **"SpiMem" conflates SPI0 and SPI1.** `cache_fctrl` and `ctrl` exist at the same offset (0x03C,
  0x008) in both the C6's `SPI0` and `SPI1` PAC blocks; the emulator's `periph::spimem` module
  is a single synthetic flash-controller model rather than two independently addressed
  peripherals, so Table A cannot tell which physical block backs a given hit — only that it's the
  flash-SPI path.
- **`esp32c6::extmem`'s "MMU entry[n]" dump is not bus traffic.** It fires exactly 3,235 times in
  both the 8 s boot-only trace and the 120 s full walk, i.e. once, at cache init — it is the
  emulator narrating its own internal MMU-table state (really backed by SPI0's
  `mmu_item_content`/`mmu_item_index` registers) rather than logging discrete reads/writes.

## Table B — Registers per block (blocks that matter to our own emulator)

Only offsets actually observed (statically or dynamically) are listed. "Dyn R/W" is blank where
the offset was never seen in either trace.

| Block | Offset | PAC name | R/W | Static sites | Dyn R | Dyn W | First-touching function |
|---|---|---|---|---:|---:|---:|---|
| UART0 | 0x02C | `hwfc_conf` | RW | - | 1 | 1 | `esp_hal::uart` (spike link init) |
| UART0 | 0x03C | `swfc_conf0` | RW | - | 2 | 2 | `esp_hal::uart` |
| UART0 | 0x048 | `idle_conf` | RW | - | 1 | 1 | `esp_hal::uart` |
| UART0 | 0x05C | `at_cmd_char` | RW | - | 802 | - | `esp_hal::uart` async RX poll |
| UART0 | 0x064 | `tout_conf` | RW | - | 804 | 2 | `esp_hal::uart` async RX poll |
| UART0 | 0x088 | `clk_conf` | RW | - | - | 1 | `esp_hal::uart` |
| UART0 | 0x098 | `reg_update` | W (self-clears) | - | 50 | 25 | `esp_hal::uart` config commit |
| USB_DEVICE | 0x000 | `ep1` | RW | 3 | - | - | `esp_println::Printer::write_bytes` |
| USB_DEVICE | 0x004 | `ep1_conf` | RW | 3 | - | - | `esp_println::Printer::write_bytes` |
| USB_DEVICE | 0x00C | `int_st` | R | 1 | - | - | `usb_serial_jtag` async interrupt handler |
| USB_DEVICE | 0x010 | `int_ena` | RW | 2 | - | - | `usb_serial_jtag` async interrupt handler |
| USB_DEVICE | 0x014 | `int_clr` | W | 1 | - | - | `usb_serial_jtag` async interrupt handler |
| USB_DEVICE | 0x018 | `conf0` | RW | 4 | - | - | button driver `open`, RMT WS281x driver `open` |
| RMT | 0x03C | `int_st` | R | 1 | - | - | `shared_driver::rmt_isr` |
| RMT | 0x040 | `int_ena` | RW | 2 | - | - | `adopt_channel` |
| RMT | 0x044 | `int_clr` | W | 2 | - | - | `rmt_isr`, `Ws281xOutput::write` |
| RMT | 0x068 | `sys_conf` | RW | 2 | - | - | `boot_firmware` |
| RMT | 0x070 | `ref_cnt_rst` | W | 2 | - | - | `Ws281xOutput::write` |
| TIMG0 | 0x064 | `wdtwprotect` | W | 2 | - | - | `Wdt<TIMG0>::new` |
| TIMG0 | 0x068 | `rtccalicfg` | RW | 6 | - | - | `configure_timg_calibration_clock`, `esp_hal::init` |
| TIMG0 | 0x080 | `rtccalicfg2` | RW | 4 | - | - | `esp_hal::init` |
| TIMG1 | 0x064 | `wdtwprotect` (TIMG1 reuses the TIMG0 register-block layout in the PAC) | W | 4 | - | - | `esp_hal::init` |
| SYSTIMER | 0x000 | `conf` | RW | 3 | - | - | `Alarm::set_enable`, `TimeDriver::arm_next_wakeup` |
| SYSTIMER | 0x004 | `unit_op` | RW | 24 | - | - | `esp_radio::common_adapter::{semphr_take,queue_send_to_front}` |
| SYSTIMER | 0x040 | `unit_value` | R | 12 | - | - | `esp_radio::common_adapter` |
| SYSTIMER | 0x044 | `unit1_value` | R | 24 | - | - | `esp_radio::common_adapter`, `Instant::now` |
| SYSTIMER | 0x06C | `int_clr` | W | 1 | - | - | `Timer::clear_interrupt` |
| INTPRI | 0x000 | `cpu_int_enable` | RW | 4 | - | - | `esp_radio::wifi::os_adapter::{ints_on,ints_off}` |
| INTPRI | 0x090 | `cpu_intr_from_cpu` | RW | 3 | - | - | `SoftwareInterrupt<0>::raise`, `arch_specific::swint_handler` |
| INTPRI | 0x094 | (unnamed — reserved padding before `date` at 0x0A0) | RW | 2 | - | - | `__pender` (embassy waker) |
| INTPRI | 0x098 | (unnamed — reserved padding) | RW | 2 | - | - | `__pender` |
| INTPRI | 0x09C | (unnamed — reserved padding) | RW | 2 | - | - | `__pender` |
| PLIC_MX | 0x000 | `mxint_enable` | RW | 2 | - | - | `boot_firmware` |
| PLIC_MX | 0x004 | `mxint_type` | RW | 2 | - | - | `_setup_interrupts` |
| PLIC_MX | 0x008 | `mxint_clear` | RW | 2 | - | - | `handle_interrupts` |
| PLIC_MX | 0x090 | `mxint_thresh` | RW | 3 | - | - | `boot_firmware`, `change_current_runlevel` |
| INTERRUPT_CORE0 | 0x000 | `core_0_intr_map(0)` (77-entry array, 0x00-0x134) | W | 3 | - | - | `_setup_interrupts`, `esp_radio::os_adapter_chip_specific::set_isr` |
| INTERRUPT_CORE0 | 0x008 | `core_0_intr_map(2)` | W | 2 | - | - | `set_isr`, `RadioRefGuard::drop` |
| INTERRUPT_CORE0 | 0x078 | `core_0_intr_map(30)` | R | 1 | - | - | `esp_hal::init` |
| INTERRUPT_CORE0 | 0x0C0 | `core_0_intr_map(48)` | W | 1 | - | - | `io_task` embassy task poll |
| INTERRUPT_CORE0 | 0x0CC | `core_0_intr_map(51)` | W | 1 | - | - | `boot_firmware` |
| INTERRUPT_CORE0 | 0x134 | `core_0_intr_status(0)` (3-entry array, 0x134-0x140; `clock_gate` itself is at 0x140, not observed) | R | 1 | - | - | `handle_interrupts` |
| GPIO | 0x028 | `enable1` | W | 1 | - | - | button driver `open` |
| GPIO | 0x03C | `in_` (GPIO input-level register) | R | 1 | - | - | `ButtonInput::poll` |
| GPIO | 0x05C | `pcpu_int` | R | 1 | - | - | default GPIO interrupt handler |
| IO_MUX | 0x000 | `pin_ctrl` | RW | - | - | - | (never resolved statically or dynamically) |
| IO_MUX | 0x044 | `gpio(16)` | RW | - | 5 | 5 | esp-hal pad-init sweep (`esp_hal::init`) |
| IO_MUX | 0x048 | `gpio(17)` | RW | - | 5 | 5 | esp-hal pad-init sweep |
| IO_MUX | 0x04C | `gpio(18)` | RW | - | 3 | 3 | esp-hal pad-init sweep + WS281x `Esp32C6RmtWs281xDriver::open` (real output pin) |
| IO_MUX | 0x050 | `gpio(19)` | RW | - | 1 | 1 | esp-hal pad-init sweep |
| IO_MUX | 0x054 | `gpio(20)` | RW | - | 1 | 1 | esp-hal pad-init sweep |
| IO_MUX | 0x058 | `gpio(21)` | RW | - | 1 | 1 | esp-hal pad-init sweep |
| IO_MUX | 0x05C | `gpio(22)` | RW | - | 1 | 1 | esp-hal pad-init sweep |
| IO_MUX | 0x060 | `gpio(23)` | RW | - | 1 | 1 | esp-hal pad-init sweep |
| IO_MUX | 0x064 | `gpio(24)` | RW | - | 4 | 4 | esp-hal pad-init sweep |
| IO_MUX | 0x068 | `gpio(25)` | RW | - | 4 | 4 | esp-hal pad-init sweep |
| IO_MUX | 0x06C | `gpio(26)` | RW | - | 4 | 4 | esp-hal pad-init sweep |
| IO_MUX | 0x074 | `gpio(28)` | RW | - | 4 | 4 | esp-hal pad-init sweep |
| IO_MUX | 0x078 | `gpio(29)` | RW | - | 4 | 4 | esp-hal pad-init sweep |
| IO_MUX | 0x07C | `gpio(30)` | RW | - | 4 | 4 | esp-hal pad-init sweep |
| PCR | 0x000 | `uart(0).conf` | RW | - | 5 | 5 | `esp_hal::init` |
| PCR | 0x004 | `uart(0).clk_conf` | RW | - | 7 | 8 | `esp_hal::init` |
| PCR | 0x00C | `uart(1).conf` | RW | - | 1 | 1 | `esp_hal::init` |
| PCR | 0x018 | `mspi_conf` | RW | - | 3 | 3 | `PeripheralClockControl` |
| PCR | 0x01C | `mspi_clk_conf` | RW | - | 6 | 7 | `PeripheralClockControl`, `esp_hal::init` |
| PCR | 0x02C | `rmt_conf` | RW | - | 4 | 4 | `<esp_hal::rmt::ChannelGuards>::new` |
| PCR | 0x030 | `rmt_sclk_conf` | RW | - | 3 | 3 | `ChannelGuards::new` |
| PCR | 0x040 | `timergroup(0).timer_clk_conf` | RW | - | 5 | 5 | `esp_hal::init` |
| PCR | 0x044 | `timergroup(0).wdt_clk_conf` | RW | - | 3 | 3 | `esp_hal::init` |
| PCR | 0x048 | `timergroup(1).conf` | RW | - | 1 | 1 | `esp_hal::init` |
| PCR | 0x054 | `systimer_conf` | RW | - | 2 | 2 | `esp_hal::init` |
| PCR | 0x080 | `saradc_conf` | RW | - | 3 | 4 | `esp_hal::init` |
| PCR | 0x084 | `saradc_clkm_conf` | RW | - | 3 | 4 | `esp_hal::init` |
| PCR | 0x088 | `tsens_clk_conf` | RW | - | 3 | 3 | `esp_hal::init` |
| PCR | 0x08C | `usb_device_conf` | RW | - | 1 | 1 | `PeripheralClockControl::enable_forced_with_counts` |
| PCR | 0x0BC | `gdma_conf` | RW | - | 1 | 1 | `PeripheralClockControl` (GDMA's *only* trace in this walk) |
| PCR | 0x0C8 | `aes_conf` | RW | - | 1 | 1 | `esp_hal::init` |
| PCR | 0x0CC | `sha_conf` | RW | - | 5 | 5 | `PeripheralClockControl` |
| PCR | 0x0E0 | `ds_conf` | RW | - | 3 | 3 | `esp_hal::init` |
| PCR | 0x0E4 | `hmac_conf` | RW | - | 3 | 3 | `esp_hal::init` |
| PCR | 0x0FC | `trace_conf` | RW | - | 1 | 1 | `esp_hal::init` |
| PCR | 0x100 | `assist_conf` | RW | - | 5 | 5 | `esp_hal::init` |
| PCR | 0x104 | `cache_conf` | RW | - | 6 | 6 | `esp_hal::init` |
| PCR | 0x110 | `sysclk_conf` | RW | - | 11 | 3 | `esp_hal::init` (boot clock-source select — the example from esp-emu's own trace-line docs) |
| PCR | 0x118 | `cpu_freq_conf` | RW | - | 5 | 3 | `esp_hal::init` |
| PCR | 0x130 | `ctrl_tick_conf` | RW | - | 3 | 3 | `esp_hal::init` |
| PCR | 0xFFC | `date` | R | - | 6 | - | `esp_hal::init` version probe |
| EFUSE | 0x034 | `rd_repeat_data1` | R | 2 | - | - | `Rwdt::set_timeout` |
| EFUSE | 0x038 | `rd_repeat_data2` | R | 1 | - | - | `Rwdt::set_timeout` |
| EFUSE | 0x044 | `rd_mac_spi_sys_0` | R | 2 | - | - | `esp_hal::efuse::base_mac_address`, `esp_phy_efuse_get_mac` |
| EFUSE | 0x048 | `rd_mac_spi_sys_1` | R | 3 | - | - | `boot_firmware`, `esp_phy_efuse_get_mac` |
| EFUSE | 0x04C | `rd_mac_spi_sys_2` | R | 1 | - | - | `boot_firmware` |
| EFUSE | 0x050 | `rd_mac_spi_sys_3` | R | 4 | - | - | `major_chip_version`/`minor_chip_version` |
| EFUSE | 0x054 | `rd_mac_spi_sys_4` | R | 2 | - | - | `major_chip_version`/`minor_chip_version` |
| EFUSE | 0x410 | (unnamed — beyond the PAC's efuse register file) | R/W | - | 8 | 8 | dynamic trace only, function not resolvable from the static scan |
| EFUSE | 0x414 | (unnamed — beyond the PAC's efuse register file) | R | - | 8 | - | dynamic trace only |
| EFUSE | 0x418 | (unnamed — beyond the PAC's efuse register file) | R | - | 8 | - | dynamic trace only |
| EFUSE | 0x440 | (unnamed — beyond the PAC's efuse register file) | RW | - | 1 | 1 | dynamic trace only |
| EFUSE | 0x444 | (unnamed — beyond the PAC's efuse register file) | RW | - | 1 | 1 | dynamic trace only |
| LP_WDT/LP_AON/PMU | see Table A rows | — | — | — | — | — | (all offsets already itemized in Table A; not repeated here for space) |
| SPI0/SPI1 (EXTMEM's real backing store) | 0x008 | `ctrl` | RW | - | 3,080 | 141 | Mask-ROM flash driver |
| SPI0/SPI1 | 0x03C | `cache_fctrl` | RW | - | 3,034 | 3,034 | Mask-ROM flash driver (cache-vs-flash-access arbitration, polled almost every flash transaction) |
| SPI0/SPI1 | 0x0A4 | `sus_status`(SPI1) | RW | - | 2 | 2 | Mask-ROM flash driver (suspend/resume) |

## Table C — esp-emu unhandled addresses

All 30 distinct addresses that ever produced a `Periph read/write unhandled C6: <addr>` line in
`traceE.emu.stderr` (626 lines total), named via the PAC map:

| Address | Named as | R count | W count | Notes |
|---|---|---:|---:|---|
| 0x600AF018 | MODEM_LPCON `clk_conf` | 55 | 55 | |
| 0x600B2414 | LP_I2C_ANA_MST `device_en` | 46 | 46 | |
| 0x600B2400 | LP_I2C_ANA_MST `i2c0_ctrl` | 43 | 43 | |
| 0x600A981C | MODEM_SYSCON `wifi_bb_cfg` | 49 | 11 | |
| 0x600AF010 | MODEM_LPCON `i2c_mst_clk_conf` | 27 | 27 | |
| 0x600B27FC | LP_I2C_ANA_MST `date` | 23 | 23 | |
| 0x600C2074 | ASSIST_DEBUG `cpu(0).debug_mode` | 22 | 0 | stack-guard/debug-monitor readback |
| 0x600B2408 | LP_I2C_ANA_MST `i2c0_data` | 20 | 0 | |
| 0x600B1C54 | LP_WDT (undocumented, reserved padding at +0x054) | 9 | 9 | not a documented register |
| 0x600AF020 | MODEM_LPCON `clk_conf_power_st` | 5 | 5 | |
| 0x600AF00C | MODEM_LPCON `wifi_lp_clk_conf` | 5 | 5 | |
| 0x600A9814 | MODEM_SYSCON `clk_conf1` | 5 | 5 | |
| 0x6009007C | IO_MUX `gpio(30)` | 5 | 5 | |
| 0x60090068 | IO_MUX `gpio(25)` | 5 | 5 | |
| 0x60090048 | IO_MUX `gpio(17)` | 5 | 5 | |
| 0x600A980C | MODEM_SYSCON `clk_conf_power_st` | 4 | 4 | |
| 0x60090078 | IO_MUX `gpio(29)` | 4 | 4 | |
| 0x60090074 | IO_MUX `gpio(28)` | 4 | 4 | |
| 0x6009006C | IO_MUX `gpio(26)` | 4 | 4 | |
| 0x60090064 | IO_MUX `gpio(24)` | 4 | 4 | |
| 0x60090044 | IO_MUX `gpio(16)` | 4 | 4 | |
| 0x6009004C | IO_MUX `gpio(18)` | 3 | 3 | the WS281x driver's actual output pin |
| 0x600C2044 | ASSIST_DEBUG `cpu(0).rcd_en` | 0 | 2 | |
| 0x600AF014 | MODEM_LPCON `modem_32k_clk_conf` | 1 | 1 | |
| 0x60090060 | IO_MUX `gpio(23)` | 1 | 1 | |
| 0x6009005C | IO_MUX `gpio(22)` | 1 | 1 | |
| 0x60090058 | IO_MUX `gpio(21)` | 1 | 1 | |
| 0x60090054 | IO_MUX `gpio(20)` | 1 | 1 | |
| 0x60090050 | IO_MUX `gpio(19)` | 1 | 1 | |
| 0x600C2048 | ASSIST_DEBUG `cpu(0).rcd_pdebugpc` | 1 | 0 | |

(Offset math for the IO_MUX rows: `gpio(n)` sits at `0x04 + 4n` in the 31-entry array, so e.g.
`0x6009007C` is `n = (0x7C-0x04)/4 = 30`, i.e. `gpio(30)`.)

## Table D — unimplemented CSRs

| CSR | R count | W count | Identity |
|---|---:|---:|---|
| 0x7A0 (`tselect`) | 35 | 35 | RISC-V debug trigger-select register |
| 0x7A1 (`tdata1`) | 35 | 35 | RISC-V debug trigger data 1 (match-control) |
| 0x7A2 (`tdata2`) | 35 | 34 | RISC-V debug trigger data 2 (compare value) |
| 0x7A5 (`tcontrol`) | 33 | 33 | RISC-V debug trigger global control |
| 0x800 | 1 | 1 | Espressif custom CSR, identity not confirmed from the material available — written once at boot with value `0x1`, immediately followed by 0x801 with the same value; no symbol in `fw-default.nm` resolves a `dedicated_gpio` or similar feature at that PC, so this is left as **custom, unknown** rather than guessed |
| 0x801 | 1 | 1 | as above — **custom, unknown** |

All four `0x7A*` CSRs are accessed together (matched read+write pairs, ~33-35 times each) —
consistent with `esp-rtos`'s stack-guard watchpoint being armed and re-armed on every context
switch using the RISC-V trigger-module CSRs (`tselect`/`tdata1`/`tdata2`/`tcontrol`), independent
of the ASSIST_DEBUG hardware block in Table A/C, which appears to be a second, unrelated
mechanism for the same job.

## Table E — ROM stubs intercepted

| Stub | Count |
|---|---:|
| `usb_serial_tx_one_char` | 231 |
| `rom_i2c_writeReg_Mask` | 183 |
| `ets_printf` | 23 |
| `rom_i2c_readReg_Mask` | 14 |
| `usb_serial_tx_flush` | 9 |
| `uart_tx_flush` | 4 |
| `ets_install_putc1` | 1 |

`usb_serial_tx_one_char` dominates even though this walk's host link is UART0 (via
`spike_uart0_link`) — the spike's `rom_tx_bytes` tee (see `spike_uart0.rs`) deliberately routes
through the *mask ROM's* `uart_tx_one_char`, and esp-emu's ROM-stub interception layer has that
export registered under its legacy name `usb_serial_tx_one_char` regardless of which physical
UART it targets. `rom_i2c_{write,read}Reg_Mask` are the PHY/RF calibration blob's low-level
analog-register accessors (the same code paths that produce the `I2C_ANA_MST`/`LP_I2C_ANA_MST`
static and dynamic hits above), intercepted at the ROM-function level rather than left to fall
through to raw MMIO tracing.

## What esp-emu's trace cannot show

These blocks are genuinely touched by the firmware (per the static scan) but esp-emu has no
per-peripheral trace tag for them, and none of their addresses ever surfaced as a chip-unhandled
hit either — meaning the access is accepted by *some* backing model, just without narration. The
dynamic column in Tables A/B is blank for these because the **tool** stays silent, not because
the **firmware** stopped touching the peripheral:

| Block | Static sites (default image) |
|---|---:|
| RMT | 9 |
| TIMG0 | 12 |
| TIMG1 | 4 |
| SYSTIMER | 64 |
| GPIO | 3 |
| INTPRI | 13 |
| PLIC_MX | 9 |
| INTERRUPT_CORE0 | 9 |
| USB_DEVICE (USB-Serial-JTAG) | 14 |
| I2C_ANA_MST | 56 |
| HP_APM | 1 |
| LP_APM | 1 |
| LP_APM0 (genuine offset only) | 1 |
| GDMA | 0 (unexercised in this walk; see Table A) |

IO_MUX's low pads (GPIO0-15, offsets 0x00-0x40) are presumed to be in this same "silent but
modeled" category by elimination — they are the only IO_MUX addresses never observed as
chip-unhandled, but no static or dynamic hit lands on one directly either, so this is inferred
rather than confirmed.

Separately, three blocks appear **modeled and logged for a curated subset of operations, but
that logging does not cover most of the raw register traffic underneath**: `periph::wifi_mac`
recognizes "INT clear" and "RX config" but the ~1,000+ static sites into the same address range
never surface individually; `periph::rtc_cntl` and `periph::system_reg` log every offset they
recognize (which turned out to be all of them, once mapped through the PAC), so those two are
fully accounted for despite the legacy tag names.

## Most surprising findings

1. **The spike exists because of a real esp-emu gap, documented in the firmware source itself.**
   the main report's §4 shows through the gdb stub that esp-emu's USB-Serial-JTAG asserts SOF
   permanently and reports EP1 "data free" forever, so the firmware believes a host is attached
   while every byte written vanishes — which is why this
   walk's entire host link had to move to a raw UART0 driver instead of the shipped
   USB-Serial-JTAG path. USB_DEVICE's Table A row (14 static sites, zero dynamic evidence, never
   unhandled) is consistent with "silently accepts writes and does nothing observable."
2. **Four real peripheral blocks touched during ordinary boot are 100% unhandled by esp-emu
   0.42.0's ESP32-C6 model**: MODEM_SYSCON, MODEM_LPCON, LP_I2C_ANA_MST, and ASSIST_DEBUG. Every
   single dynamic access to any of them — reads and writes alike — falls through to the generic
   "Periph unhandled C6" catch-all (reads return 0, writes are dropped). The walk still completed
   successfully (shader compiled, WS281x output opened, 60 s of rendering ran), so none of this
   firmware's boot path depends on those registers behaving correctly — but any future feature
   that checks a status bit from PHY calibration's analog I2C bus or the modem clock-gate block
   would be checking a register that can never report anything but 0.
3. **IO_MUX's pad-config model stops at GPIO15 — and GPIO18, the actual RMT/WS281x output pin
   this walk exercises, is past that boundary.** `IO_MUX_GPIOn_REG` for n=0..15 never appears as
   unhandled; n=16..30 always does, GPIO18 included. The pad-mux write that routes the RMT
   peripheral's signal onto GPIO18 is silently dropped by the emulator, meaning the WS281x
   driver's own pad-routing step has no modeled effect at all under esp-emu, independent of
   whatever RMT itself does with the data.

## Appendix — distilled trace (top 120 of 500 distinct lines)

Values, addresses, lengths, and PC/cycle counters are stripped and lines are sorted by
frequency, via:

```
grep -E '^\[(TRACE|DEBUG)' walks/traceE.emu.stderr \
  | sed -E 's/= 0x[0-9A-Fa-f]+/= 0xNN/; s/0x[0-9A-Fa-f]{6,8}/0xADDR/g; \
      s/PC=0x[0-9A-Fa-f]+ cycles=[0-9]+ insns=[0-9]+/PC=.. cycles=.. insns=../; \
      s/addr=0x[0-9A-Fa-f]+/addr=0xADDR/g; s/len=[0-9]+/len=N/g; \
      s/page=[0-9]+/page=N/g; s/entry\[[0-9]+\]/entry[N]/g' \
  | sort | uniq -c | sort -rn
```

Run over the full 50,484-line `traceE.emu.stderr`, this produces 500 distinct lines — over the
~300-line inclusion threshold for this report, so only the top 120 (by count) are reproduced
below; re-run the command above against `walks/traceE.emu.stderr` for the rest.

```
13528 [DEBUG periph::spimem] SPI USR cmd: 0x0003
13528 [DEBUG periph::spimem] SPI USR READ addr=0xADDRR len=N
5103 [DEBUG periph::spimem] SPI FLASH_RDSR
3080 [TRACE periph::spimem] SpiMem read unhandled offset: 0x008
3072 [DEBUG periph::esp32c6::extmem] MMU entry[N] = 0xNN (page=N, valid=false)
3034 [TRACE periph::spimem] SpiMem write unhandled offset: 0x03C = 0xNN
3034 [TRACE periph::spimem] SpiMem read unhandled offset: 0x03C
 804 [TRACE periph::uart] UART read offset 0x064
 802 [TRACE periph::uart] UART read offset 0x05C
 664 [DEBUG periph::spimem] SPI FLASH_WREN
 634 [DEBUG periph::spimem] SPI USR cmd: 0x0002
 634 [DEBUG periph::spimem] SPI USR PP addr=0xADDRR len=N
 347 [TRACE periph::esp32c6::chip] Periph read unhandled C6: 0xADDR
 279 [TRACE periph::esp32c6::chip] Periph write unhandled C6: 0xADDR = 0xNN
 231 [TRACE machine::rom_stubs] ROM stub 'usb_serial_tx_one_char' at 0xADDR
 183 [TRACE machine::rom_stubs] ROM stub 'rom_i2c_writeReg_Mask' at 0xADDR
 161 [DEBUG periph::esp32c6::extmem] MMU entry[N] = 0xNN (page=N, valid=true)
 142 [TRACE periph::spimem] SpiMem write unhandled offset: 0x008 = 0xNN
  50 [TRACE periph::uart] UART read offset 0x098
  35 [TRACE cpu::csr] CSR write to unimplemented register 0x7A1 = 0xNN
  35 [TRACE cpu::csr] CSR write to unimplemented register 0x7A0 = 0xNN
  35 [TRACE cpu::csr] CSR read from unimplemented register 0x7A2
  35 [TRACE cpu::csr] CSR read from unimplemented register 0x7A1
  35 [TRACE cpu::csr] CSR read from unimplemented register 0x7A0
  34 [TRACE cpu::csr] CSR write to unimplemented register 0x7A2 = 0xNN
  33 [TRACE cpu::csr] CSR write to unimplemented register 0x7A5 = 0xNN
  33 [TRACE cpu::csr] CSR read from unimplemented register 0x7A5
  29 [DEBUG periph::spimem] SPI FLASH_SE addr=0xADDRR
  25 [TRACE periph::uart] UART write offset 0x098 = 0xNN
  23 [TRACE machine::rom_stubs] ROM stub 'ets_printf' at 0xADDR
  23 [DEBUG machine] Patching ROM stub at 0xADDR with C.EBREAK
  14 [TRACE machine::rom_stubs] ROM stub 'rom_i2c_readReg_Mask' at 0xADDR
  12 [TRACE periph::rtc_cntl] RTC_CNTL read offset 0x154
  11 [TRACE periph::system_reg] SYSTEM read offset 0x110 = 0xNN
  11 [DEBUG machine] Loading ROM section at 0xADDR (4 bytes)
  10 [TRACE periph::rtc_cntl] RTC_CNTL write offset 0x154 = 0xNN
   9 [TRACE machine::rom_stubs] ROM stub 'usb_serial_tx_flush' at 0xADDR
   8 [TRACE periph::system_reg] SYSTEM write offset 0x004 = 0xNN
   8 [TRACE periph::rtc_cntl] LP_AON write offset 0x02C = 0xNN
   8 [TRACE periph::rtc_cntl] LP_AON read offset 0x02C
   8 [TRACE periph::efuse] eFuse write offset 0x410 = 0xNN
   8 [TRACE periph::efuse] eFuse read offset 0x418
   8 [TRACE periph::efuse] eFuse read offset 0x414
   8 [TRACE periph::efuse] eFuse read offset 0x410
   7 [TRACE periph::system_reg] SYSTEM write offset 0x01C = 0xNN
   7 [TRACE periph::system_reg] SYSTEM read offset 0x004 = 0xNN
   7 [TRACE periph::rtc_cntl] RTC_CNTL write offset 0x0AC = 0xNN
   7 [TRACE periph::rtc_cntl] RTC_CNTL read offset 0x400
   7 [TRACE periph::rtc_cntl] RTC_CNTL read offset 0x0AC
   6 [TRACE periph::system_reg] SYSTEM write offset 0x104 = 0xNN
   6 [TRACE periph::system_reg] SYSTEM read offset 0xFFC = 0xNN
   6 [TRACE periph::system_reg] SYSTEM read offset 0x104 = 0xNN
   6 [TRACE periph::system_reg] SYSTEM read offset 0x01C = 0xNN
   6 [TRACE periph::rtc_cntl] RTC_CNTL write offset 0x0CC = 0xNN
   6 [TRACE periph::rtc_cntl] RTC_CNTL read offset 0x0CC
   5 [TRACE periph::system_reg] SYSTEM write offset 0x100 = 0xNN
   5 [TRACE periph::system_reg] SYSTEM write offset 0x0CC = 0xNN
   5 [TRACE periph::system_reg] SYSTEM write offset 0x040 = 0xNN
   5 [TRACE periph::system_reg] SYSTEM write offset 0x000 = 0xNN
   5 [TRACE periph::system_reg] SYSTEM read offset 0x118 = 0xNN
   5 [TRACE periph::system_reg] SYSTEM read offset 0x100 = 0xNN
   5 [TRACE periph::system_reg] SYSTEM read offset 0x0CC = 0xNN
   5 [TRACE periph::system_reg] SYSTEM read offset 0x040 = 0xNN
   5 [TRACE periph::system_reg] SYSTEM read offset 0x000 = 0xNN
   5 [TRACE periph::rtc_cntl] RTC_CNTL write offset 0x420 = 0xNN
   5 [TRACE periph::rtc_cntl] RTC_CNTL read offset 0x420
   4 [TRACE periph::system_reg] SYSTEM write offset 0x084 = 0xNN
   4 [TRACE periph::system_reg] SYSTEM write offset 0x080 = 0xNN
   4 [TRACE periph::system_reg] SYSTEM write offset 0x02C = 0xNN
   4 [TRACE periph::system_reg] SYSTEM read offset 0x02C = 0xNN
   4 [TRACE periph::rtc_cntl] RTC_CNTL write offset 0x400 = 0xNN
   4 [TRACE periph::rtc_cntl] RTC_CNTL write offset 0x0DC = 0xNN
   4 [TRACE periph::rtc_cntl] RTC_CNTL write offset 0x0D0 = 0xNN
   4 [TRACE machine::rom_stubs] ROM stub 'uart_tx_flush' at 0xADDR
   3 [TRACE periph::wifi_mac] WiFi INT clear: 0xADDR, remaining: 0xADDR
   3 [TRACE periph::system_reg] SYSTEM write offset 0x130 = 0xNN
   3 [TRACE periph::system_reg] SYSTEM write offset 0x118 = 0xNN
   3 [TRACE periph::system_reg] SYSTEM write offset 0x110 = 0xNN
   3 [TRACE periph::system_reg] SYSTEM write offset 0x0E4 = 0xNN
   3 [TRACE periph::system_reg] SYSTEM write offset 0x0E0 = 0xNN
   3 [TRACE periph::system_reg] SYSTEM write offset 0x088 = 0xNN
   3 [TRACE periph::system_reg] SYSTEM write offset 0x044 = 0xNN
   3 [TRACE periph::system_reg] SYSTEM write offset 0x030 = 0xNN
   3 [TRACE periph::system_reg] SYSTEM write offset 0x018 = 0xNN
   3 [TRACE periph::system_reg] SYSTEM read offset 0x130 = 0xNN
   3 [TRACE periph::system_reg] SYSTEM read offset 0x0E4 = 0xNN
   3 [TRACE periph::system_reg] SYSTEM read offset 0x0E0 = 0xNN
   3 [TRACE periph::system_reg] SYSTEM read offset 0x088 = 0xNN
   3 [TRACE periph::system_reg] SYSTEM read offset 0x084 = 0xNN
   3 [TRACE periph::system_reg] SYSTEM read offset 0x080 = 0xNN
   3 [TRACE periph::system_reg] SYSTEM read offset 0x044 = 0xNN
   3 [TRACE periph::system_reg] SYSTEM read offset 0x030 = 0xNN
   3 [TRACE periph::system_reg] SYSTEM read offset 0x018 = 0xNN
   3 [TRACE periph::spimem] SpiMem write unhandled offset: 0x014 = 0xNN
   3 [DEBUG machine] Loading ROM section at 0xADDR (8 bytes)
   2 [TRACE periph::wifi_mac] WiFi RX config: enabled=true, dma_base=0x15DBC
   2 [TRACE periph::wifi_mac] WiFi RX config: enabled=false, dma_base=0x15DBC
   2 [TRACE periph::uart] UART write offset 0x064 = 0xNN
   2 [TRACE periph::uart] UART write offset 0x03C = 0xNN
   2 [TRACE periph::uart] UART read offset 0x03C
   2 [TRACE periph::system_reg] SYSTEM write offset 0x050 = 0xNN
   2 [TRACE periph::system_reg] SYSTEM read offset 0x120 = 0xNN
   2 [TRACE periph::system_reg] SYSTEM read offset 0x11C = 0xNN
   2 [TRACE periph::system_reg] SYSTEM read offset 0x050 = 0xNN
   2 [TRACE periph::spimem] SpiMem write unhandled offset: 0x0A4 = 0xNN
   2 [TRACE periph::spimem] SpiMem write unhandled offset: 0x010 = 0xNN
   2 [TRACE periph::spimem] SpiMem read unhandled offset: 0x0A4
   2 [TRACE periph::spimem] SpiMem read unhandled offset: 0x010
   2 [TRACE periph::rtc_cntl] RTC_CNTL write offset 0x41C = 0xNN
   2 [TRACE periph::rtc_cntl] RTC_CNTL write offset 0x418 = 0xNN
   2 [TRACE periph::rtc_cntl] RTC_CNTL write offset 0x164 = 0xNN
   2 [TRACE periph::rtc_cntl] RTC_CNTL write offset 0x0E4 = 0xNN
   2 [TRACE periph::rtc_cntl] RTC_CNTL write offset 0x074 = 0xNN
   2 [TRACE periph::rtc_cntl] RTC_CNTL write offset 0x028 = 0xNN
   2 [TRACE periph::rtc_cntl] RTC_CNTL read offset 0x41C
   2 [TRACE periph::rtc_cntl] RTC_CNTL read offset 0x418
   2 [TRACE periph::rtc_cntl] RTC_CNTL read offset 0x164
   2 [TRACE periph::rtc_cntl] RTC_CNTL read offset 0x0E4
   2 [TRACE periph::rtc_cntl] RTC_CNTL read offset 0x028
   2 [DEBUG periph::spimem] SPI USR cmd: 0x0035
```
