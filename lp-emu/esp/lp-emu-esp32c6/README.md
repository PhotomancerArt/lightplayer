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
**not** reproduce — the partition table, the MMU page table, the ROM's console
globals, the `rst:0x1 (POWERON)` banner, early RNG entropy, real eFuse, and
the derived reset cause. That list is the seed for M7's cross-check, and it is
worth more than the code around it.

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
| `PCR` | `0x6009_6000` | accept | `sysclk_conf.clk_xtal_freq` pinned 40; `cpu_waiti_conf.cpu_wait_mode_force_on` pinned 0; `timergroup.timer_clk_conf` reset = XTAL; `uart(n).clk_conf` drives the two UART clock lines (reset = XTAL, enabled) |
| `I2C_ANA_MST` | `0x600A_F800` | accept | `ana_conf0.cal_done` pinned 1; `i2c_ctrl(0/1).busy` pinned 0; `ana_conf2` reset 0 (master 1) |
| `LP_I2C_ANA_MST` | `0x600B_2400` | accept | `i2c0_ctrl.I2C0_BUSY` (bit 25) pinned 0 — the bench's fourth spin site |
| `ASSIST_DEBUG` | `0x600C_2000` | accept | `cpu0.debug_mode` pinned 0 (no debugger: watchpoints arm, `wfi` runs) |
| `GPIO` | `0x6009_1000` | accept | `in_` and `pcpu_int` pinned 0; `enable`/`out` writes are trace lines (pins: M5) |
| `IO_MUX` | `0x6009_0000` | accept | all 31 pads at reset `0x0800` |
| `PMU`, `LP_AON`, `LP_APM`, `LP_APM0`, `HP_APM`, `MODEM_SYSCON`, `MODEM_LPCON`, `APB_SARADC`, `HP_SYS`, `TEE`, `LP_TEE`, `LP_IO`, `LP_TIMER`, `EXTMEM` | — | accept | written by `esp_hal::init`, read back as written; `LP_AON.store1` carries the calibration value |
| `UART0`, `UART1` | `0x6000_0000/1000` | modelled | 128-byte FIFOs; the shifter drains **at the configured baud in emulated time** (PCR clock line × `clkdiv`; reset `clkdiv = 347 + 3/16` = 115,200 from XTAL, *modeled* "as the ROM boot leaves it"); `rxfifo_full`/`txfifo_empty` as levels (`>`/`<` the `conf1` thresholds, per the TRM), `rxfifo_tout` in bit-times, `tx_done`, `rxfifo_ovf`, `reg_update` pulse; `at_cmd_char_det` never fires (stated, not modelled); sources 43/44. See "UART0 and the outside" |
| `USB_DEVICE` | `0x6000_F000` | modelled (M6 P2) | the host's side in three states (`--usb-host absent\|attached\|attached-idle`, the transitions for P3's control channel): **absent** — `sof` never, `free` = 0 for ever after the first `wr_done`, nothing arrives; **attached, port closed** — `int_raw.sof` every 1 ms (*documented*), `fram_num` counts, a committed IN packet is held until the port opens; **attached, draining** — the packet reaches the `usb-sj` stream 100 µs after `wr_done` (*modeled*), `free` returns, `serial_in_empty` and `in_token_rec_in_ep1` rise; host bytes land as ≤ 64 B OUT packets, one resident at a time (*modeled*), `avail` + `serial_out_recv_pkt` + `out_ep1_st.wr_addr/rec_data_cnt`. The DTR/RTS dance → `chip_rst` bit 0 + `MachineRequest::Reset { strap }`. Per-register grades: `fram_num`, `conf0` *documented*, the rest *modeled* (the file header's table; `--strict-grade`). The PCR reset of the block is **not** modelled (stated). Source 48 |
| `SPI0`, `SPI1` | `0x6000_2000/3000` | accept | a flash access spins on `SPI1.cmd` (the `SPIN` line names it) until M4 |
| `RMT` | `0x6000_6000` | accept | `Rmt::new` runs in every image; channels, blocks and the WS281x waveform are M5 |
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

The block carries the first **per-register grade table** (`fram_num` and
`conf0` *documented*; everything else *modeled* until P4 promotes what the
transcripts cover). `--strict-grade documented` refuses the first access to
a register below that grade — on this machine that is the first MMIO access
of the boot, since every accept table is *modeled* — and reports it as a
strict-bus stop with the grade in the message. The level is a claim about
what the run trusts, not a switch that makes the machine more accurate.

Not modelled, stated: the PCR reset of the block on esp-hal's first enable
(whether it re-enumerates on silicon is the sitting-1 transcript's to say),
the JTAG channel, line coding, the bus-error and zero-payload bits.

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
raised: nothing here receives, and the `WIFI RX config` line is where the
virtual-air work (M6) will start.

## Reference images and the gates

The committed silicon transcript and the spike report's figures are at
firmware `d6cfaa205` with the `spike_uart0_link` feature applied as a dirty
tree. `scripts/emu/build-reference-image.sh <features>` reproduces that
tree in a detached worktree under `target/emu-ref/` and builds it there:

```bash
scripts/emu/build-reference-image.sh test_shader_compile_incremental,esp32c6,spike_uart0_link   # harness
scripts/emu/build-reference-image.sh esp32c6,server,radio,spike_uart0_link,memory_fs            # boot-idle-memfs
scripts/emu/build-reference-image.sh esp32c6,server,radio,spike_uart0_link                      # boot-idle (flash-backed)
```

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

The driver builds nothing this machine does not need: it turns a payload into
one invocation of the CLI above — `--elf`, `--time-grade`, `--uart0 file:`,
`--exit-on`, `--timeout`, `--strict-bus`, `--efuse-mac`, `--efuse-rev` — so
the plan a `--dry-run` prints is the entire protocol, and the sidecar records
it verbatim as `source`. `--image` is per payload because a set runs several
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
pinned.

## Tests

`cargo test -p lp-emu-esp32c6` runs everything that needs no firmware. The
boot tests are `#[ignore]`d, because a workspace test run must not start a
cross-target firmware build; `just test-emu-c6` sets the environment and runs
them. `test_support` resolves the ELF from `LP_EMU_C6_ELF_<SLUG>`, then the
conventional target path, and only builds when `LP_EMU_BUILD_FW=1`. The
reference-image tests (`harness_parity`, `boot_idle`) resolve theirs from
`LP_EMU_C6_REF_<SLUG>`, then `target/emu-ref/`, and with `LP_EMU_BUILD_FW=1`
run `scripts/emu/build-reference-image.sh` — which needs the repository's
history for the reference commit (a shallow CI checkout cannot do it), so it
is a local affair.

`just test-emu-c6` is the whole set: the machine's boot tests, the M3 replays
of the committed transcripts, and the registry parity test. About **70 s** on
a warm cargo cache with the reference images absent — roughly 25 s of firmware
build and 45 s of emulation. The replays alone
(`cargo test -p lp-emu-validate --test m3_replays`) need no firmware at all
and already run in `cargo test`, so the four gates cost CI nothing.

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
