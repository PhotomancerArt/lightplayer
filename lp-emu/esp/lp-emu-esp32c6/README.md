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

P6 was the second candidate, and the answer was the same. The brief
pre-approved hooks for the ROM console path (`uart_tx_one_char`,
`uart_tx_flush`, `ets_get_printf_channel`) in case the ROM routed by a
global its boot would have set. It does route by one — `ets_printf_uart`, a
ROM `.bss` byte — and that byte is 0 after a direct load, which is UART0. So
the spike image's tee and esp-println's `uart` printer reach the real ROM
`uart_serial_tx_one_char`, which spins on `status.txfifo_cnt` and stores to
the real FIFO, and the table is still empty. What P6 did add is the ROM's
**initialised data** (`rom::seed_data`): the non-allocated `PROGBITS`
sections of the ROM ELF are what the ROM startup copies into HP SRAM and
what the mask ROM physically holds past its last `PT_LOAD`; without them
`pp_rom_version` is NULL and the WiFi blob faults inside `_vsnprintf`.

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
**not** reproduce — the partition table, the MMU page table's *provenance*,
the ROM's console globals, the `rst:0x1 (POWERON)` banner, early RNG entropy,
real eFuse, and the derived reset cause. That list is the seed for M7's
cross-check, and it is worth more than the code around it.

Two of those seven the loader now does, because M4's firmware asks the flash
*chip* questions and the two halves of the address space have to describe one
board:

- **`stage_image_in_flash`** puts the image's flash-resident segments into
  the chip at `paddr = 0x0001_0000 + (vaddr - 0x4200_0000)` — the `factory`
  partition's offset plus the offset into the window, which is a whole number
  of 64 KiB pages, so the cache MMU's `paddr % page == vaddr % page` holds —
  and programs the page table for them. It is **not** an `esptool` image
  layout; M7 boots a real merged image through the real bootloader and gets
  the real offsets, and this is one of the things that cross-check checks.
- **`seed_rom_flash_chip`** writes the chip size into
  `rom_spiflash_legacy_data->chip_size`, in place of the bootloader's
  `esp_rom_spiflash_config_param`. The ROM's own default chip is **2 MiB**
  (`rom_default_spiflash_legacy_data` at `0x4087_fa08`), `SPI_read_data`
  refuses any read past `chip_size`, and `lpfs` starts at `0x0031_0000` —
  so without this every filesystem read returns error 1 for a reason that has
  nothing to do with the filesystem.

## Flash, and the cache window

The chip is a `flash::FlashImage`: read, program (an `&=`, because a NOR cell
only goes one to zero — programming twice without an erase corrupts here
exactly as it does on the part), erase back to `0xff`, and a JEDEC id whose
capacity byte is the image's real size. Where its bytes come from is
`FlashBacking`:

| flag | at start | at exit |
|---|---|---|
| *(none)* | a blank chip, all `0xff` | nothing |
| `--flash <file>` | the file, `0xff`-padded (created if absent) | written back |
| `--flash-copy <file>` | the file | nothing |

`--flash-size` takes a power of two of at least 64 KiB; anything else is
refused, because the JEDEC capacity byte is an exponent and there is no
honest id for a chip that is not one.

The `0x4200_0000` window reads **through the MMU**. `cache::CacheMmu` is the
page table, and every number in it is read off the vendored ROM ELF rather
than a datasheet: `Cache_MMU_Init` (`0x4002_7c76`) says 256 entries and that
zero is invalid, `Cache_MSPI_MMU_Set` (`0x4002_7c90`) says an entry is
`page | encrypt<<10 | VALID<<9` and gives the index arithmetic,
`MMU_Get_Page_Mode` (`0x4002_75ea`) says the page mode is
`mmu_power_ctrl[4:3]`. `CacheMmu::translate` is the whole address path in one
function, on purpose: a later `t2` rung hangs its cache-miss wait states off
exactly that lookup.

The window is served as a **cache fill** — `cache::fill` copies a valid
page's flash bytes into the RAM region behind `0x4200_0000` when the table
changes or the flash under a mapped page is written — so instruction fetch
stays a RAM read. That makes the model *stricter than silicon about
staleness*: a real cache keeps serving old bytes until something invalidates
it, and this one refills at the next slice boundary. Nothing in this
milestone's images writes a mapped page (the app is in `factory`, littlefs in
`lpfs`, and only `lpfs` is written), so the difference is documented rather
than exercised.

The run summary reports what the guest asked the flash to do, for comparison
with the spike inventory's esp-emu figures:

```text
flash: 300 commands (287 reads, 4 page programs, 2 sector erases, 0 block
erases, 7 write-enables; 60 status polls); 37 page(s) filled into the cache
window
```

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
| `PCR` | `0x6009_6000` | accept | `sysclk_conf.clk_xtal_freq` pinned 40; `cpu_waiti_conf.cpu_wait_mode_force_on` pinned 0; `timergroup.timer_clk_conf` reset = XTAL; `uart(n).clk_conf` drives the two UART clock lines (reset = XTAL, enabled); `rmt_conf`/`rmt_sclk_conf` drive the RMT clock line (reset `0x0050_1000` = PLL/2 = 40 MHz; esp-hal writes `div_num 0` for 80) |
| `I2C_ANA_MST` | `0x600A_F800` | accept | `ana_conf0.cal_done` pinned 1; `i2c_ctrl(0/1).busy` pinned 0; `ana_conf2` reset 0 (master 1) |
| `LP_I2C_ANA_MST` | `0x600B_2400` | accept | `i2c0_ctrl.I2C0_BUSY` (bit 25) pinned 0 — the bench's fourth spin site |
| `ASSIST_DEBUG` | `0x600C_2000` | accept | `cpu0.debug_mode` pinned 0 (no debugger: watchpoints arm, `wfi` runs) |
| `GPIO` | `0x6009_1000` | modelled (M5 P2) | a routing **view** over the bus's signal fabric: `func_out_sel_cfg[n]` routes pad `n` to `out_sel` (128 = follow `GPIO_OUT[n]`, `inv_sel` inverts, `oen_sel` recorded and reported as `oe=`, never gated on), `out`/`out_w1ts`/`out_w1tc` are the output bitmap a `GPIO_OUT` pad follows, `enable`/`w1ts`/`w1tc` the OE bitmap. The `w1ts`/`w1tc` registers fold into `out`/`enable` and read back 0 (write-only in the PAC); `in_` and `pcpu_int` still pinned 0 — nothing drives a pad from outside and no GPIO interrupt is ever pending. Everything else is still a `RegFile`. A pad is **observed once the guest writes its routing**: seeding 31 routes from the `0x80` reset value would give a boot that drives nothing 31 pads to decode. See "The pin" |
| `IO_MUX` | `0x6009_0000` | accept | all 31 pads at reset `0x0800` |
| `PMU`, `LP_AON`, `LP_APM`, `LP_APM0`, `HP_APM`, `MODEM_SYSCON`, `MODEM_LPCON`, `APB_SARADC`, `HP_SYS`, `TEE`, `LP_TEE`, `LP_IO`, `LP_TIMER`, `EXTMEM` | — | accept | written by `esp_hal::init`, read back as written; `LP_AON.store1` carries the calibration value |
| `UART0`, `UART1` | `0x6000_0000/1000` | modelled | 128-byte FIFOs; the shifter drains **at the configured baud in emulated time** (PCR clock line × `clkdiv`; reset `clkdiv = 347 + 3/16` = 115,200 from XTAL, *modeled* "as the ROM boot leaves it"); `rxfifo_full`/`txfifo_empty` as levels (`>`/`<` the `conf1` thresholds, per the TRM), `rxfifo_tout` in bit-times, `tx_done`, `rxfifo_ovf`, `reg_update` pulse; `at_cmd_char_det` never fires (stated, not modelled); sources 43/44. See "UART0 and the outside" |
| `USB_DEVICE` | `0x6000_F000` | measured on its data path (M6) | the host's side in three states (`--usb-host absent\|attached\|attached-idle`, the transitions for P3's control channel): **absent** — `sof` never, `free` = 0 for ever after the first `wr_done`, nothing arrives; **attached, port closed** — `int_raw.sof` every 1 ms (*documented*), `fram_num` counts, a committed IN packet is held until the port opens; **attached, draining** — the packet reaches the `usb-sj` stream 100 µs after `wr_done` (*modeled*), `free` returns, `serial_in_empty` and `in_token_rec_in_ep1` rise; host bytes land as ≤ 64 B OUT packets, one resident at a time (*modeled*), `avail` + `serial_out_recv_pkt` + `out_ep1_st.wr_addr/rec_data_cnt`. The DTR/RTS dance → `chip_rst` bit 0 + `MachineRequest::Reset { strap }`. Per-register grades (the file header's table; `--strict-grade`): `ep1`, `ep1_conf` and the four `int_*` registers *measured* — four committed transcripts cover them, and the bits they cover are named there — `fram_num` and `conf0` *documented*, the twenty listed below *modeled*. The PCR reset of the block is **not** modelled (stated). Source 48 |
| `SPI1` | `0x6000_3000` | modelled | **the legacy flash controller**, against a `flash::FlashImage`: `flash_rdid` (esp-storage's own size probe), the `usr` engine (command/address/dummy/data phases from `user`/`user1`/`user2`/`addr`/`w0..w15`), the dedicated `flash_read`/`pp`/`se`/`be`/`ce`/`wren`/`wrdi`/`rdsr`/`wrsr` bits, and a real status register (WIP always clear, WEL set by `wren` and consumed by a program or erase). Every trigger self-clears and `mst_st` reads idle, which is what `Wait_SPI_Idle` waits for. **Every PAC reset value is carried**, `user = 0x8000_0000` above all: the mask ROM's read path never sets `usr_command` because reset already did |
| `SPI0` | `0x6000_2000` | modelled | the cache controller's block: `mmu_item_content`/`mmu_item_index`/`mmu_power_ctrl` drive `cache::CacheMmu`; the rest accept, with the PAC's reset values |
| `RMT` | `0x6000_6000` | modelled (M5 P1) | the PAC register file, the **192-word RAM** at `+0x400` (word/half/byte lanes, read live by the engine), and two TX engines on the scheduler: a word's two pulses at PCR's function clock (`rmt_sclk_conf` × `div_cnt`; one tick = 2 cycles, a WS2812 bit 200, the latch 48,000 — exact integer arithmetic over absolute ticks, anchored on the previous due cycle), **`tx_lim` as a position** (`== window_words` is the wrap), `mem_tx_wrap_en`, the all-zero STOP → `tx_end`, wrap off → `tx_err` + `mem_empty`, `int_st = raw & ena`, `int_clr` w1c, source 49. *Modeled* (discovery §10.1–5, each named where made): `mem_raddr_ex` = the next word to fetch; the strobes act at `conf_update` (a `tx_start` with no `conf_update` in the slice is acted on at the slice boundary, noted); the half-level end marker; `tx_stop` raises no `tx_end`; `ref_cnt_rst` accepted; no clock stalls the engine (1 ms poll resumes it), FOSC refused. RX channels 2/3 accept-and-remember (`rx_en` is noted once); the APB FIFO is not modelled. Since P2 the waveform is **driven onto the bus's signal fabric** as `RMT_SIG_0 + ch` at every pulse start, with the idle level at end/stop; `rmt_pulses`/`rmt_words` are the word-level oracle a gate reads and are off unless `Esp32C6Builder::rmt_logs(true)` asks for them. See "The RMT chase" and "The pin" |
| `WIFI_MAC` | `0x600A_0000..9800` | accept (*modeled*) | the radio window as **one** block with coarse names (`mac` / `ieee802154` / `bb` — ours, nothing documents it), a `TOUCH` note per distinct offset, and the override list `wifi_stub::OVERRIDES` (five entries, one per `SPIN` the boot showed, each with the poll's disassembly beside it); `+0x4084` is the RX DMA base the `WIFI RX config` line reports |
| `WIFI_PWR` | `0x600A_9900..F000` | accept | the undocumented gap after MODEM_SYSCON (the ROM's `tsf_hal_*` touch it first); `+0x3700` is a live **microsecond counter** (*modeled* `cycles / 160`) — the blob's `wait_i2c_sdm_stable` latches it and gives up after 9,999 ticks, and a remembered 0 never lets it |
| `I2C_MST_MEM` | `0x600A_FC00..600B_0000` | accept | the analog I2C master's burst **command memory**, `I2C_ANA_MST + 0x400`, which the PAC's block (ending at `date`, `+0x34`) does not cover; libphy's `phy_i2c_master_cmd_mem_init` fills it and nothing reads it back |

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
- `WIFI_MAC TOUCH +0x0418 mac (R; 27 distinct so far)` — the first access
  to each distinct offset of a coarse-named block; a run's log is the list
  of what the blob reached.
- `WIFI RX config: dma_base=0x4081557c` — the blob programming its RX
  descriptor ring (`mac_rxbuf_init`), the first artefact of the virtual-air
  work.
- `USB_DEVICE wr_done: 28 bytes committed to the IN endpoint; no host will
  drain them` / `… the host is attached but not draining (port closed)` /
  `… the host takes them in 100 us`, then `IN packet of 64 bytes delivered
  to the host`, `OUT packet of 5 bytes landed`, `ep1 write with the IN FIFO
  committed (host attached-idle): byte 0x0a dropped`, `host attached`,
  `port opened`, `chip reset from the serial channel, strap = download` —
  the USB model narrating the host's side.
- `STRICT-GRADE read of USB_DEVICE+0x000 ep1: graded modeled, the run
  trusts documented and above` — the one line a `--strict-grade` stop
  leaves before the report.
- `RMT ch0 start f_rmt=80000000 div_cnt=1 window=0..192 raddr=0 wrap=1
  tx_lim=96`, `RMT ch0 thr pos=96`, `RMT ch0 end words=6146 idle=0`,
  `RMT ch0 err mem_empty (window end, wrap off)`, `RMT ch0 stop idle=0`,
  `RMT ch0 stalled: …` / `resumed` — the TX engines (M5 P1); `--trace RMT`
- `PIN gpio18 <- RMT_SIG_0 (out_sel=71 inv=0 oen_sel=0 oe=1)` /
  `PIN gpio16 <- GPIO_OUT (out_sel=128 …)` — one note per routing *change*
  from the GPIO view (M5 P2); esp-hal rewriting the same routing on a
  rebind is not a note
  adds the register and RAM writes around them, which is how a refill's
  timing against the read pointer is read off.

A 3 s no-radio run with `--trace SYSTIMER,PLIC_MX,INTPRI,INTERRUPT_CORE0,LP_WDT,TIMG0`
is about 780 k lines, half of them the RTC-calibration poll at boot. Without
a filter it is 18 M lines; write it to a file on a disk with room.

### UART0 and the outside

TX bytes leave the shifter one symbol apart at the configured baud and are
written to the host stream *as they leave*, so a capture's byte order is
the guest's order at the guest's rate. That is what makes the ROM's
blocking `uart_tx_one_char` wait like silicon once `txfifo_cnt` reaches
128, and it is why the harness transcript's ticks 19–92 show silicon's
7 ms-per-line floor (see "Reference images" below): the time class is
reported against silicon, never compared, and this is the first place the
model spends time the way the chip does.

Where the bytes go is `--uart0`: `stdout`, `file:<path>`, `memory`, or
`tcp:<host:port>`, which **listens** for one client at a time (as esp-emu's
`--uart-tcp` did for the spike's proxy) and takes the client's bytes as
UART0's RX — `lp-cli … serial:tcp://127.0.0.1:5555` connects to it, the P7
seam. Where RX bytes come from otherwise is `--uart0-script <file>`: one
chunk per line, `<ms>` then a double-quoted string (`\n \r \t \xNN`) or hex
bytes, delivered at that emulated millisecond one byte per symbol.

**A run with a live socket is not deterministic.** A socket's bytes arrive
at whatever cycle the 1 ms poll landed on when the host wrote them; two
runs with the same client are two different runs. The script is the
deterministic path, and the determinism tests use it (or no input at all).

### USB-Serial-JTAG and the host

The model is the **host's side** of the shipped image's link
(`periph/usb_sj.rs` says what each register does in each state and what
the two drivers then do). `--usb-host` picks the state at power-on; the
transitions (`attach`, `detach`, `open`, `close`, the DTR/RTS dance) are
methods on the block that M6 P3's control channel will drive.

- **absent** (the default, P6's machine): no SOF, `free` never returns
  after the first `wr_done`. esp-println fills 64 bytes, commits them,
  spins 50,000 times on `ep1_conf.serial_in_ep_data_free`, latches
  `TIMED_OUT` and falls silent; `UsbConnectionMonitor` never sees a SOF, so
  the connected path that arms `int_ena.serial_out_recv_pkt` is never
  taken. `tests/host_absent.rs`.
- **attached-idle** (a cable, no application reading): SOF every 1 ms, so
  the firmware believes it is connected, but a committed packet is never
  taken. The first `[INIT]` line sits in the endpoint for the whole run;
  the hello's first 64-byte chunk is dropped into the committed FIFO and
  its write waits out 250 ms, a log line's leading newline waits out
  another, the monitor latches "not draining", and from then on the only
  IN traffic is one `\n` probe every 2 s — the vehicle-neutral signature
  `tests/usb_attached.rs` reads from the trace (on the shipped image:
  143 ms, 393 ms, 643 ms, then 2,143 ms, 4,143 ms). `TIMED_OUT` = 1 and
  the RX path never armed, as with no host: io_task is sequential and
  never reaches `read_serial` while its writes time out.
- **attached** (a host draining): each `wr_done` packet reaches the
  `usb-sj` stream 100 µs later (*modeled*: sub-millisecond, under every
  timeout the firmware uses), `serial_in_empty` fires and esp-hal's write
  future completes through the ISR; host bytes (a scripted source today,
  P3's socket next) land as ≤ 64 B OUT packets one at a time. The boot
  lines, the hello, the heartbeats: 2.9 KB in 5.5 s, `TIMED_OUT` never
  set.

Two logs, because the sink's meaning changed with the host: `--usb-sj`
(`Esp32C6Machine::usb_sj`) is **what a host received**, and is empty with
no host or the port closed; `--usb-sj-tried` (`usb_sj_tried`) is the
observation stream — what the guest handed over that nobody took (pushed
with no host, dropped into a committed FIFO, dropped by a bus reset).
`--probe esp_println::serial_jtag_printer::TIMED_OUT@3000` reads the
printer's latch in any state.

#### The per-register grades, and what `--strict-grade` means

The block carries the chip's first **per-register grade table**, and M6 P4
promoted the half of it the transcripts cover:

| grade | registers |
|---|---|
| `measured` | `ep1`, `ep1_conf`, `int_raw`, `int_st`, `int_ena`, `int_clr` |
| `documented` | `fram_num` (the USB full-speed frame is 1 ms), `conf0` (the PAC's bit map; the shipped image never writes it on the C6) |
| `modeled` | `test`, `jfifo_st`, `in_ep0_st`, `in_ep1_st`, `in_ep2_st`, `in_ep3_st`, `out_ep0_st`, `out_ep1_st`, `out_ep2_st`, `misc_conf`, `mem_conf`, `chip_rst`, `set_line_code_w0`, `set_line_code_w1`, `get_line_code_w0`, `get_line_code_w1`, `config_update`, `ser_afifo_config`, `bus_reset_st`, `date` |

The four transcripts under `lp-emu/transcripts/esp32c6/` are what moved the
first row: SOF present while attached and gone when the cable is out, `free`
returning only once a host has drained the packet, `serial_in_empty`
completing esp-hal's write future, `serial_out_recv_pkt` on the host's own
bytes. Three of those registers are measured **bit by bit** — bits 1–3 of
the `int_*` group and bits 0–2 of `ep1_conf` — and the file header says which
bits and which are not; grading them with the register they live in is
coarser than the evidence, and that is written down rather than smoothed
over.

The third row has one reason for all twenty: **neither esp-hal 1.1.1 nor
esp-println 0.17 touches them on the C6**. Their reset values are the PAC's
and reads answer them; nothing behind them is modelled. So the shipped image
runs 5.5 s attached under `--strict-grade documented` and crosses none of
them (`tests/usb_attached.rs`, G4-4), and a change that starts reading one
stops the run with the register's name.

**The level applies to the blocks that publish a table**, and today that is
this one. A block with no table is passed over, because "nobody graded this
block" is not the same statement as "this block is modelled" — conflating
them made `documented` stop at the first MMIO access of any boot, on an
accept table nobody had said anything about, and the flag then measured how
much of the chip had been graded rather than what the run was allowed to
trust. The run report names the blocks it checked (`strict-grade documented:
checked 1 (USB_DEVICE)`), so an ungraded block reads as an unanswered
question and never as a pass. Grading the accept tables is the
honest-peripheral policy's next milestone, not a gap this flag hides.

Not modelled, stated: the PCR reset of the block on esp-hal's first enable
(sitting 1's attached-host transcript shows both the `[INIT]` lines and the
hello arriving on one port open, which says it does **not** re-enumerate on
silicon — so the seam stays unbuilt, with the evidence recorded), the JTAG
channel, line coding, the bus-error and zero-payload bits.

#### Driving the host from outside (M6 P3)

`--usb-host` is only the state at power-on. The transitions are commands,
on a socket (`--control tcp:<host:port>`) or in a file (`--usb-script`),
and the protocol is documented once in `../README.md`:

```bash
# a byte client and a control client on one machine
just emu-c6 <elf> --usb-host attached-idle \
    --usb-sj tcp:127.0.0.1:5556 --control tcp:127.0.0.1:5557 --timeout 60s
# the s7 unplug, deterministically
just emu-c6 <elf> --usb-script scripts/s7.usb --usb-sj file:cap.log --timeout 12s
```

Three things are worth knowing before reading that section:

- **Connecting to the byte socket opens the port**, and disconnecting
  closes it (`--usb-sj-drain manual` hands both to the control channel).
  That is what makes `lp-cli … serial:tcp://<addr>` work unchanged against
  `--usb-host attached-idle`.
- **`attach`/`detach` are never implied by a socket.** A cable is not a
  port open — the whole reason for modelling the host is that the two come
  apart.
- **A script is deterministic; a socket is not.** A command from a socket
  is applied at whichever slice boundary the poll landed on, and the reply
  says which guest cycle that was. Gates use scripts
  (`tests/usb_control.rs`); the socket has one plumbing test
  (`tests/usb_socket.rs`).

What that buys, and what `tests/usb_control.rs` records: a port held closed
after a re-attach makes the firmware commit a packet nobody takes, drop
what it writes behind it, and — when the port opens — deliver the held
packet and then log `[io_task] host draining again; resuming protocol
writes`. Its twin, `host not draining`, never appears on the link: it is
queued and then dropped by the very latch it reports (M6 discovery §4).
That asymmetry is the thing G3's desk sitting exists to measure, and a
control channel reproduces the recovery half of it with no rig.

### The RMT chase

The WS281x driver (`lp-ws281x`, through `fw-esp32c6`'s `c6_rmt.rs`) fills
both halves of a channel's RAM window, arms `tx_lim` at the half, starts,
and then races the transmitter: every `tx_thr_event` the ISR reads
`mem_raddr_ex`, flips `tx_lim` between the half and the whole window,
plants a STOP guard in the half just left and refills it. The model is the
consumer side of that race in cycles — a word's pulses are scheduled at
`anchor + cycles_for(absolute ticks since tx_start)`, so a bit is exactly
200 cycles wherever a slice boundary falls, the RAM is read at the fetch
(a late refill is overwritten data, as on the chip), and the threshold is
a **position** in the window (`periph/rmt.rs` says why the alternative
was refuted on silicon).

The M5 P1 gates (`tests/rmt_chase.rs`, `--strict-bus`, `t1`): the
`esp32c6,server,test_rmt` harness — a 256-LED white chase on GPIO18 through
the one-channel plan (192-word window, 96-word halves) — for 800 ms ends
24 frames, each **6,144 data words + latch + STOP**, every data word a
WS2812 code, the bits decoding to pixel `k mod 256` white and the rest
black, 64 `thr` per frame, words 200 cycles apart and the latch 48,000; two
runs are identical, and `t2` sends the same words. The first frame starts
at ≈ 359 ms because the harness's logger pays a 250 ms drain timeout on
the host-absent USB link first. Refill lag is *reported* from the trace
(P3 adds the histogram), never gated (plan D13/PD9).

What the machine exposes: `rmt_pulses(ch)` (every level/duration with its
start cycle), `rmt_words(ch)` (every fetched word with its cycle, STOPs
included) and `rmt_frames_ended(ch)`. The first two are the **word-level
oracle** the pin gate compares its decoder against, and they are off unless
a builder asks (`rmt_logs(true)`): a 24-frame run holds 305,490 pulses, and
since P2 the waveform reaches a pad anyway.

### The pin

`GPIO.func_out_sel_cfg[18] = 71` is written by esp-hal's `with_pin`
(preceded by `out_w1tc` bit 18, `IO_MUX.gpio18.mcu_sel = 1` and
`enable_w1ts` bit 18 — M5 discovery §2a, §8), and that write is what makes
the RMT's channel-0 waveform reach gpio18. The GPIO block writes it into
the bus's **signal fabric** (`lp-emu-esp-common`'s `pins`, plan DD34 e);
the RMT drives signal `71 + ch` into the same fabric; neither sees the
other. Every slice the machine drains the fabric's edges into one WS281x
decoder per routed pad and, optionally, a raw pin log.

```text
cyc=57437313 PIN gpio18 <- RMT_SIG_0 (out_sel=71 inv=0 oen_sel=0 oe=1)
```

Flags (`--trace GPIO,RMT` shows the routing and the engine notes):

| flag | what it does |
|---|---|
| `--dump-frames stdout\|file:<path>` | one JSON line per decoded frame as it completes: `{"kind":"ws281x-frame","pad":18,"signal":"RMT_SIG_0","n":0,"start_us":359136.931,"end_us":366816.081,"bits":6144,"leds":256,"wire":"0a0a0a00…","rgb":"0a0a0a00…","errors":0,"trailing_bits":0,"reset_us":10416.475,"complete":true}` |
| `--strip-order grb\|rgb\|rbg\|gbr\|brg\|bgr` | the order `rgb` is unpermuted with (default `grb`) — `wire` is always what the wire carried |
| `--strip-timing ws2812\|ws2811` | the wire timing a pad is decoded against (default `ws2812`) |
| `--pin-log file:<path>` | every edge, `<us> gpio18 0\|1`; 12,288 lines per 256-LED frame, capped at 2,000,000 |

At exit each routed pad gets a line on stderr:
`pin gpio18: 24 frames, 24 complete, 0 errors, 256 leds`.

**What is observed:** the level of a routed pad, and the cycle it changed.
A pad becomes observed when the guest writes its `func_out_sel_cfg` — the
memfs boot image routes exactly one, `init_board`'s plain GPIO output on
gpio16, and no peripheral signal at all.

**What is not:** inputs (`in_` reads 0), output enable (`oen_sel` and
`enable` are recorded and printed in the routing note, never gated on: a
pad whose OE is low still records its edges), drive strength, pull-ups,
open-drain, pad filters, and `IO_MUX.mcu_sel` — esp-hal writes `mcu_sel = 1`
before every route and `IO_MUX` stays an accept block, so a pad routed here
carries its signal whatever `mcu_sel` says. A word's two pulses reach the
fabric together at the fetch, each stamped with the cycle it starts, so an
edge can be recorded up to one word (200 cycles) ahead of the slice
boundary — a timestamp, never a reordering.

The M5 P2 gate (`tests/rmt_chase.rs`): on the chase image the trace carries
one routing note and it is gpio18's, and every frame the decoder reads off
that pad is **byte-equal to the frame P1's word log describes** — 24 frames
of 6,144 bits, 0 errors, the chase pixel where it belongs, a 10.4 ms reset
between frames and an 18.1 ms frame period. Two runs write byte-identical
`--dump-frames` files, and a snapshot taken with the decoder mid-bit
restores to decode the same frames.

### The radio window

Everything esp-radio's blob and the ROM's PHY code touch between
`0x600A_0000` and `0x600B_0000` is three accept-and-remember blocks
(`periph/wifi_stub.rs`), and the rule for them is plan D5's: run the
image, read every `SPIN` line as a status bit the hardware would have set,
disassemble the poll, add the one override that lets it exit with the
evidence beside it, run again. The boot asked five times —

| where | register | the poll | reads |
|---|---|---|---|
| `txdc_cal_new` | `WIFI_MAC+0x418` | `slli a3,a4,9; bgez` — bit 22, done | bit 22 = 1 |
| `ram_pwdet_tone_start` / `_wait_idle` | `WIFI_MAC+0x814` | `srli 14; andi 7; bne 7` — a state field | bits 14:16 = 7 |
| `ram_set_chan_freq_sw_start` | `WIFI_MAC+0x0cc` | `andi 256; beqz` — bit 8, lock | bit 8 = 1 |
| `rom_iq_est_enable` (ROM) | `WIFI_MAC+0x4a0` | `slli a3,a5,15; bgez` — bit 16, done | bit 16 = 1 |
| `hal_init` | `WIFI_MAC+0x4ddc` | `andi 1; beqz` — bit 0, ready | bit 0 = 1 |

— and then said hello. The list is `wifi_stub::OVERRIDES`; a unit test walks
it and refuses an entry without a reason. Interrupt sources 0–3 are never
raised: nothing here receives, and the `WIFI RX config` line is where
virtual-air work would start.

## Reference images and the gates

The M2/M3 silicon transcript and the spike report's figures are at firmware
`d6cfaa205` with the `spike_uart0_link` feature applied as a dirty tree.
`scripts/emu/build-reference-image.sh <features> [<commit>] [<spike>|none]`
reproduces a tree in a detached worktree under `target/emu-ref/` and builds
it there:

```bash
scripts/emu/build-reference-image.sh test_shader_compile_incremental,esp32c6,spike_uart0_link   # harness
scripts/emu/build-reference-image.sh esp32c6,server,radio,spike_uart0_link,memory_fs            # boot-idle-memfs
scripts/emu/build-reference-image.sh esp32c6,server,radio,spike_uart0_link                      # boot-idle (flash-backed)
# M6: the shipped image over its OWN link, at the commit the desk board ran
scripts/emu/build-reference-image.sh esp32c6,server,radio,memory_fs 735af98ae none
```

`none` (or `--no-spike`) in the third slot builds the commit's own tree with
no cherry-pick at all, and it is what M6 needs: the shipped image speaks its
real USB-Serial-JTAG link, so the UART0 workaround would not merely be
unnecessary, it would be a different image — and the DD30 arbitration is only
an arbitration if both sides are the same bytes. A no-spike build leaves the
worktree clean, so `build.rs` stamps `dirty: false` the way a silicon flash
of that commit does.

The ELF's sha256 is written beside it. It is **per worktree** — the build
path is in the binary — so two checkouts' images differ in bytes while the
code they carry does not; the harness gate, which is byte-equality of the
*guest's* memory figures, is what says the code is the same.

The M3 P6 gates, all `--strict-bus`, `t1`, default eFuse identity, no host
input (`tests/{harness_parity,boot_idle,host_absent}.rs`):

- **G6-1**, the harness replayed against
  `lp-emu/transcripts/esp32c6/shader-compile-stress/silicon-…-d6cfaa205.txt`
  with M2's runner: memory **372/372 equal** (all 184 per-tick values,
  peak 48,132, resident 18,932, after-drop 3,976), structural 190/190,
  timing reported — `build_us` 550,958 vs 568,757 (**0.97×**),
  `max_slice_us` 10,676 vs 11,724 (0.91×), per-tick `slice_us` sum 0.97×.
  esp-emu was 10.5× fast on the same ticks because its ROM UART drained for
  free; here the drain is modelled at baud and the floor appears.
- **G6-2**, the memfs spike image: the hello frame (`proto 20`,
  `seeed/xiao-esp32-c6`, `chipRevision 0.2`, `baseMac a0:f2:62:87:b4:8c`),
  the RMT line, `ESP-NOW radio ready`, `[RECOVERY] boot complete`, then the
  heartbeat: `[stack] high-water 11432 B of 71960 B` (§5.4 exactly) and
  `freeBytes 266688` at 5 s against esp-emu's 266,792 — a transient (the
  15 s sample reads 266,788; `tests/boot_idle.rs` says what was ruled out).
  The flash-backed spike image stops at `SPIN SPI1+0x000 cmd` at 11 ms: the
  `[FS]` mount pair and its heartbeat are M4's (DD23).
- **G6-3**, the shipped image minus flash: `TIMED_OUT == 1`, `int_ena`
  bit 2 never armed, the sink holds `[INIT] Initializing board...`, 3,970
  RWDT feeds and 4,037 `wfi` skips in 5 s.
- **G6-4**: two runs of each → byte-identical UART0 captures, identical
  cycle and instruction counts.
- **G6-5**: the `WIFI RX config` line; five overrides, each with its `SPIN`;
  no radio `SPIN` left.

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
    [--uart0 stdout|memory|file:<path>|tcp:<host:port>] [--uart0-script <file>]
    [--flash <file>] [--flash-copy <file>] [--flash-size 4M]
    [--usb-host absent|attached|attached-idle]
    [--usb-sj stderr|memory|file:<path>|tcp:<host:port>] [--usb-sj-drain auto|manual]
    [--usb-sj-tried stderr|memory|file:<path>]
    [--control tcp:<host:port>] [--usb-script <file>]
    [--efuse-mac a0:f2:62:87:b4:8c] [--efuse-rev 0.2] [--seed <u64>]
    [--trace [BLOCK,BLOCK…]] [--trace-file <path>] [--strict-bus]
    [--strict-grade modeled|documented|measured]
    [--probe <symbol>@<ms>] [--break-at <symbol>] [--hooks] [--map]
```

`--exit-on` stops at the **end of the line** the match is on, not at the
match, and it watches **both** consoles — UART0 and the USB link — because
the shipped image's console is the USB one and a run with `--usb-host
attached` prints nothing on UART0 at all. A console drains a byte at a time
in emulated time, so a needle that is a prefix of its line would otherwise
end the run mid-line — which is how M3 P7 first recorded `[stack] heartbeat:
high-water` with neither of the two figures after it. If the newline never
arrives the run goes on to its deadline, which is the safe direction: a run
that ran too long says so, a capture cut in half looks like data.

`--control`, `--usb-script` and `--usb-sj tcp:` are the host's side of the
USB link. The protocol — the command table, the replies, the coupling rule,
the script grammar and the Web Serial mapping — is one section in
`../README.md`; the short version is below.

`--probe` and `--break-at` take an ELF name, a demangled path
(`esp_println::serial_jtag_printer::TIMED_OUT`, LLVM's `.N` suffix
stripped) or a unique suffix; an ambiguous one is refused, with the
alternatives in the log. `--break-at` stops at the symbol's first
instruction with every register as the caller left it and prints
`a0..a7`, `sp`, `mcause/mepc/mtval`, `a0..a3` as text when they point at
text, and the `s0` frame-pointer backtrace — inside a panic path that
chain walks the panic machinery's own frames, so break at
`ExceptionHandler` or `core::panicking::panic_fmt` for a clean one.

Every timeout is **emulated** time, so a run is the same run on a laptop and
on a loaded CI box. A duration without a unit is refused rather than guessed:
`100` could be anything, and guessing would run a thousand times too long or
too short.

Exit codes are a contract: `0` on an `--exit-on` match or a clean timeout,
`2` on a hart fault (symbolized against the app ELF) or a reset request
(the report names the strap: `app` or `download`), `3` on a strict-bus or
strict-grade violation, `4` on the wall-clock safety net, `5` on a
`--break-at`.

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

## The runner, and what a transcript costs

`just emu-c6` above is the debugging door. The door that makes a *claim* is
the validation runner, where this machine is the configuration
`lp-emu:esp32c6:t1` (and `:t2` for the other grade):

```bash
cargo run -p lp-cli -- validate run emu-m3 --config lp-emu:esp32c6:t1 --dry-run
cargo run -p lp-cli -- validate record emu-m3 --config lp-emu:esp32c6:t1 \
    --date <today> --commit d6cfaa2051ae --dirty --timeout-secs 20 \
    --image shader-compile-stress=target/emu-ref/d6cfaa205-harness/fw-esp32c6 \
    --image boot-idle=target/emu-ref/d6cfaa205-boot-idle-memfs/fw-esp32c6
```

M4 adds the set `emu-m4`, whose two payloads run the **flash-backed** image:

```bash
cargo run -p lp-cli -- validate record emu-m4 --config lp-emu:esp32c6:t1 \
    --date <today> --commit d6cfaa2051ae --dirty --timeout-secs 20 \
    --image boot-idle-flash=target/emu-ref/d6cfaa205-boot-idle/fw-esp32c6 \
    --image upload-walk=target/emu-ref/d6cfaa205-boot-idle/fw-esp32c6
```

`upload-walk` carries a `host_script` — `walks/examples-basic.script`, which
the driver passes as `--uart0-script`. That is the whole of what makes a
*walk* a payload: the host half of the conversation, recorded so the run is a
function of guest time. See `walks/README.md`.

The driver builds nothing this machine does not need: it turns a payload into
one invocation of the CLI above — `--elf`, `--time-grade`, `--uart0 file:`,
`--exit-on`, `--uart0-script`, `--timeout`, `--strict-bus`, `--efuse-mac`,
`--efuse-rev` — so the plan a `--dry-run` prints is the entire protocol, and
the sidecar records it verbatim as `source`. `--image` is per payload because a set runs several
and a reference image is built per feature set. The eFuse identity comes from
the configuration's entry in `validate.toml` (the desk board's
`a0:f2:62:87:b4:8c`, rev `v0.2`), so a hello frame's identity fields compare
equal to a silicon transcript's rather than differing over who was told what.

The sidecar's `tools` carry this crate's version and commit and the vendored
ROM's sha256. They do **not** carry a hash of the image: the build path is
compiled into the ELF, so two checkouts of the same source differ in bytes and
agree in code. What identifies the image is the recipe — script, commit,
feature set — recorded in `note`.

Every class is graded `modeled`, with byte-equality written into the reason as
evidence rather than as a promotion; `validate.toml` is where those reasons
live and `tests/m3_replays.rs` is where strict mode's refusal of them is
pinned. `tests/m4_replays.rs` reads the two M4 transcripts the same way, and
`tests/m6_replays.rs` the M6 ones — including P5's pair of walks over the
shipped link and the flash-backed DD30 arbitration against the desk board.

## Tests

`cargo test -p lp-emu-esp32c6` runs everything that needs no firmware. The
boot tests are `#[ignore]`d, because a workspace test run must not start a
cross-target firmware build; `just test-emu-c6` sets the environment and runs
them. `test_support` resolves the ELF from `LP_EMU_C6_ELF_<SLUG>`, then the
conventional target path, and only builds when `LP_EMU_BUILD_FW=1`. The
images: `SHIPPED` (the default feature set — the bytes a board is flashed
with, and since M6 P5 what every USB test runs), `NO_RADIO`
(`esp32c6,server,memory_fs`), `SHIPPED_NO_FLASH`
(`esp32c6,server,radio,memory_fs` — kept because the M6 scenario transcripts
name it, not because anything still stands in with it) and `TEST_RMT`
(`esp32c6,server,test_rmt` — the bare `esp32c6,test_rmt` does not link on
today's main, `panic_path.rs` needs `lpc_shared`). The
reference-image tests (`harness_parity`, `boot_idle`, `flash_persistence`,
`upload_walk`, `upload_walk_usb`) resolve theirs from
`LP_EMU_C6_REF_<SLUG>`, then `target/emu-ref/`, and with `LP_EMU_BUILD_FW=1`
run `scripts/emu/build-reference-image.sh` — which needs the repository's
history for the reference commit (a shallow CI checkout cannot do it), so it
is a local affair.

| file | what it holds |
|---|---|
| `harness_parity` | M3's memory gate: the compile harness replayed against silicon, in process |
| `boot_idle` | the memfs image's hello and §5.4 heartbeat, **and** the flash-backed image's `[FS]` pair and §5.1 heartbeat — M4's first gate, which replaced the test that pinned the `SPI1.cmd` spin |
| `flash_persistence` | a chip survives the machine: format once, mount twice, `--flash-copy` writes nothing |
| `upload_walk` | the thirteen-frame upload from `walks/examples-basic.script`, the second boot that auto-loads what it wrote, and determinism |
| `host_absent`, `boot_no_radio`, `boot`, `rom_*`, `stack_guard` | M3's |

`just test-emu-c6` is the whole set: the machine's boot tests, the M3 and M4
replays of the committed transcripts, and the registry parity test. About
**70 s** on a warm cargo cache with the reference images absent — roughly
25 s of firmware build and 45 s of emulation. The replays alone
(`cargo test -p lp-emu-validate --test m3_replays --test m4_replays`) need no
firmware at all and already run in `cargo test`, so the gates cost CI
nothing.

`just bench-emu-c6` is the speed side of the same two images: both reference
images at both grades, reported as user seconds, instructions/second and a
real-time ratio, with the load average and a `cmp` of the UART0 bytes against
the previous run. It builds `--release` deliberately — that is the profile
users get, and the root `Cargo.toml` lifts this crate to `opt-level = 3`
there (`lp-emu/README.md`, "Speed"). No CI job runs it and nothing gates on
what it prints.

## Provenance

- The mask ROM is Apache-2.0, from `espressif/esp-rom-elfs` release
  `20260528`. See `../roms/README.md`.
- The register-name tables in `src/regs/` are generated from the `esp32c6`
  PAC's svd2rust offset comments by `scripts/emu/pac-regnames.py` and carry
  the provenance header `docs/adr/2026-07-29-license-provenance-discipline.md`
  requires. Never hand-edit one; `just lint-emu-regnames` catches it.
- The crate is MIT, as a unit with the rest of `lp-emu/`. See
  `../../README.md` and `just lint-emu-fence`.
