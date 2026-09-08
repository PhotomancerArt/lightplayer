# Why the ESP32-C6 mask ROM's download console was silent, settled by trace

**Date:** 2026-09-08 · **Phase:** M3 P1 of the C6 rounding-out roadmap
(`lp2025/2026-09-08-0839-c6-emulator-rounding-out/m3/p1-…`) · **Branch:**
`claude/c6r-m3-p1-rom-console-sync` · **On top of:** `main` at `63e8b8256`
(debt-sweep PR #612 merged).

## The one-paragraph answer

The register model was never the problem. The real mask ROM's download
console polls `USB_DEVICE.ep1_conf` bit 2 (`serial_out_ep_data_avail`) at
`0x400228dc` and reads `ep1` at `0x400228e2`, two registers the M6 model
already drives the way silicon does. The committed proof,
`lp-emu/esp/lp-emu-esp32c6/scripts/rom-download-sync.usb`, had its two
chunks stamped `20000` and `40000` with a comment saying microseconds — but
the `--usb-script` grammar's leading number is **milliseconds**, so the SYNC
was scheduled for twenty *seconds* of emulated time and the `READ_REG` for
forty, past the end of every run that attached the file (M7 ran the console
for 400 ms; the `.usb` was never attached to a test at all). With the stamps
read as the grammar reads them (`20` and `40`), the real ROM answers the SYNC
eight times and the `READ_REG` with the run's own eFuse MAC, on the first
try, with no change to `usb_sj.rs`, no hook, and `--strict-bus` clean. The
UART0 console was a separate, real gap — the ROM will not read a byte from
UART0 until it has measured the host's baud from the pulse counters, and the
UART model had none — and that half is modelled in `periph/uart.rs` (RD9).

## What was run

Everything below is on `main` at `63e8b8256`, `lp-emu-esp32c6 --release`,
the vendored `esp32c6_rev0_rom.elf`, a **blank** 4 MiB flash (the download
console never opens the flash, so no firmware build is involved), the
download strap, `--reset-cause usb-uart`, `--usb-host attached`,
`--efuse-mac a0:f2:62:87:b4:8c`, `--strict-bus`, and
`--trace USB_DEVICE,UART0,INTPRI,PLIC_MX` (plus `INTERRUPT_CORE0` on the UART0
runs). Instruction counts are post-#612 and are not comparable with anything
recorded before it.

| run | script | timeout | result |
|---|---|---|---|
| 1 | committed `.usb` (stamps `20000`/`40000`) | 60 ms | banner only; **no OUT packet ever landed**; 184,821 trace lines, 421,279 of them `ep1_conf = 0x00000002` polls |
| 2 | the same bytes at `20`/`40` ms | 100 ms | banner, 8 × SYNC reply, READ_REG reply — 265 bytes on the USB link |
| 3 | the same bytes on `--uart0-script` (no auto-baud model) | 100 ms | banner only; `rxd_cnt` read 348 times, always 0 |
| 4 | `rom-download-sync.uart0` (two SYNCs), auto-baud model, `--strict-grade documented` | 240 ms | banner, 8 × SYNC reply, READ_REG reply — 265 bytes on UART0; `clkdiv` written `0x15c` |
| 5 | fixed `.usb`, `--strict-grade documented` | 60 ms | as run 2; `strict-grade documented: checked 3 (UART0, UART1, USB_DEVICE)`, no violation |

## G1-1: the instruction the ROM stopped at

The ROM never *stopped*; it polled. In run 1 the last 400,000 lines of the
trace are one instruction reading one register and getting one value:

```text
cyc=9599941 pc=0x400228dc R4 USB_DEVICE+0x004 ep1_conf = 0x00000002
```

- **Instruction:** `0x400228dc  lw a5, 4(a4)` in `usb_serial_device_rx_one_char`
  (`0x400228d8`), followed by `andi a5, a5, 0x4; beqz a5, 0x400228ec` — "no
  byte, return 1".
- **Register:** `USB_DEVICE.ep1_conf` (`0x6000_f004`).
- **Value read:** `0x00000002` — `serial_in_ep_data_free` set, bit 2
  `serial_out_ep_data_avail` **clear**.
- **Value needed:** bit 2 set (`0x00000006`), which the block reads once an
  OUT packet is resident, and which silicon produces when the host has
  written a bulk OUT packet the device has not yet read out.
- **Why it never came in run 1:** no host bytes were staged. The `.usb`
  script's `20000` parsed as 20,000 ms:

  ```text
  usb script: 60 byte(s) of host input in 2 chunk(s) and 0 control command(s)
  ```

  and no `OUT packet … landed` note exists anywhere in the 60 ms trace.
  Run 2, same bytes at `20`:

  ```text
  cyc=3216000 pc=0x40017600 USB_DEVICE OUT packet of 46 bytes landed (serial_out_ep_data_avail = 1, serial_out_recv_pkt raised)
  cyc=3232084 pc=0x400228dc R4 USB_DEVICE+0x004 ep1_conf = 0x00000006
  cyc=3232087 pc=0x400228e2 R4 USB_DEVICE+0x000 ep1 = 0x000000c0
  cyc=3232126 pc=0x400228e2 R4 USB_DEVICE+0x000 ep1 = 0x00000000
  cyc=3232175 pc=0x400228e2 R4 USB_DEVICE+0x000 ep1 = 0x00000008
  ```

  (20 ms + the block's `OUT_LAND_LATENCY_US`; the ROM's poll sees it on its
  next loop and pops the frame byte by byte.)

- **Evidence silicon supplies the value:** `ep1` and `ep1_conf` are graded
  `measured` in `usb_sj.rs` by the `upload-walk-usb` transcript — host
  bytes over this link, `lp-cli`'s hello and `emu_usb_hello`, replayed
  against silicon's capture — which is exactly the path the ROM takes here
  (`serial_out_ep_data_avail`, then the `ep1` pops). The `boot-idle-flash`
  silicon capture bounds the TX side of the same console (a draining host
  takes the ROM's packets fast enough that its dropping path never fires).

## The three hypotheses, against the trace

The order is F22's. Each is ruled out by the ROM's own instructions plus what
the trace shows them reading.

**h1 — the detector never selects USB because a status silicon drives is not
driven here.** Ruled out. `detect_uart_usb_spi_sdio_boot_mode` (`0x400187d6`)
reads **no** USB status at all. It is a loop of `UartConnCheck(0)` then
`UartConnCheck(3)` (`0x40023388`), and `UartConnCheck(port)` selects the
console by *receiving a valid SYNC on that port*: `uart_buff_switch(port)`
writes the console byte at `0x4087f580` (`sb s0, 0x18(a5)` with `a5 =
0x4087f568`), waits 1 ms, and calls `RcvMsg(buf, 0x2000, 1)`; a frame whose
first two bytes are `00 08` is answered with eight 12-byte SYNC responses and
the detector returns. No `fram_num`, no `in_token_rec_in_ep1`, and the ROM
global `usb_uart_connected` (`0x4087f564`) is read only by
`usb_serial_device_tx_one_char` (`0x4002285e`), on the TX side. The
USB_DEVICE census of the whole USB-console run is `ep1_conf` reads, `ep1`
reads/writes and 14 `ep1_conf` writes (`wr_done`) — nothing else:

```text
207945 R4 USB_DEVICE+0x004 ep1_conf
   265 W4 USB_DEVICE+0x000 ep1
    60 R4 USB_DEVICE+0x000 ep1
    14 W4 USB_DEVICE+0x004 ep1_conf
```

**h2 — `RcvMsg` needs the interrupt path through accept-only INTPRI, or a
`serial_out_recv_pkt` route.** Ruled out for the USB console; it is half
right for UART0 and the machine already does that half. `uart_rx_readbuff`
(`0x40022edc`) branches on the console byte: for port 3 it calls
`usb_serial_device_rx_one_char` directly — polled, no ring buffer, no
interrupt — so nothing on the USB console depends on an interrupt. INTPRI is
touched **zero** times in every run. `ets_isr_unmask(0x20)` is
`esprv_intc_int_enable` on **PLIC_MX** (`mxint_enable = 0x20`), which the
interrupt-matrix model serves, and the only route programmed is UART0's:

```text
cyc=6186 pc=0x40022aa8 W4 INTERRUPT_CORE0+0x0ac core_0_intr_map43 = 0x00000005
```

On the UART0 console that route is live and works today: the ROM's
`uart_rx_intr_handler` pulls the RX FIFO into its ring buffer at
`0x40029612` three bytes at a time (`conf1.rxfifo_full_thrhd = 1`) in run 3
— the bytes arrive; what stops UART0 is the auto-baud gate, not the
interrupt.

**h3 — the SLIP reader needs packet boundaries the script's chunking does not
provide.** Ruled out by `recv_packet` (`0x40022f70`): it is byte-oriented
(`0xC0` start/end, `0xDB 0xDC`/`0xDD` escapes) with its state and count kept
in ROM `.bss` (`0x4087f598`/`0x4087f59c`) across polls, so a frame may arrive
in any chunking. Run 2 delivers the 46-byte SYNC as one OUT packet and it is
parsed in one `UartConnCheck(3)` call; on UART0 the same frame arrives one
byte every 86.8 µs across many detector loops and is parsed all the same.

**What none of the three predicted:** the proof was never delivered. The
stamps were the cause, and the grammar's own documentation
(`rom-reset-into-download.usb`: "Times are EMULATED MILLISECONDS") had said
so from the start.

## G1-2 / G1-3: the replies, quoted

USB link, run 2 / run 5, after the 139-byte banner:

```text
c0 01 08 04 00 07 07 12 20 00 00 00 00 c0   × 8       (SYNC: dir 1, cmd 0x08, size 4, value 0x20120707, status 00 00, +2 zero bytes)
c0 01 0a 04 00 8c b4 87 62 00 00 00 00 c0             (READ_REG: value 0x6287b48c = the low four bytes of a0:f2:62:87:b4:8c)
```

With `--efuse-mac 40:4c:ca:11:22:33` the last reply is
`c0 01 0a 04 00 33 22 11 ca 00 00 00 00 c0` — the test
`a_different_efuse_mac_gives_a_different_read_reg_reply` pins that the two
logs differ in exactly those four bytes.

## G1-4: UART0, through the auto-baud model

Run 4, host baud 115,200 (the peripheral's default; `--uart0-baud` is M3
P2's, see below). The ROM enables detection, counts, reads the minima, and
writes the divisor:

```text
cyc=1937594 pc=0x40029382 UART0 auto-baud enabled: counting edges and pulse minima for a host at 115200 baud (347 sclk clocks per bit)
cyc=3518012 pc=0x400293ec R4 UART0+0x084 rxd_cnt = 0x0000008e
cyc=3518021 pc=0x400293fe R4 UART0+0x07c lowpulse = 0x0000015b
cyc=3518032 pc=0x40029410 R4 UART0+0x080 highpulse = 0x0000015b
cyc=3518054 pc=0x400293a2 W4 UART0+0x020 conf0 = 0x0014001c
cyc=3518147 pc=0x4002953a W4 UART0+0x014 clkdiv = 0x0000015c
```

`0x8e` = 142 edges > `0x7f`; `low = high = 0x15b` = 347 = ⌊40,000,000 /
115,200⌋; the ROM's `((347 + 347) << 3) + 16 = 5568` sixteenths →
`uart_hal_clk_set_div` writes `clkdiv = 348, frag = 0` (`0x15c`), i.e.
40e6 × 16 / 5568 = 114,943 baud. Nothing was chosen to land there: at a
stated 921,600 the same code gives 43 clocks a bit and `clkdiv = 44`
(`periph::uart::tests::the_auto_baud_counters_follow_the_stated_host_baud`).

The first SYNC (20 ms) is consumed by detection: `uart_div_modify` resets
both FIFOs and the detector returns while that frame is still on the wire.
The second SYNC (120 ms) is answered — the first reply byte leaves at
`cyc=19904764` (124.4 ms) — and so is the `READ_REG` (220 ms). UART0's 265
bytes are the banner plus the same 126 reply bytes quoted above. esptool
sends up to seven SYNCs per attempt for this reason; the committed
`rom-download-sync.uart0` sends two.

## G1-5: strict, both

Runs 4 and 5 (the CLI) and every test in `tests/rom_download_console.rs`:

```text
unmapped: 0 reads, 0 writes, 0 distinct sites; 0 idle skips (wfi)
strict-grade documented: checked 3 (UART0, UART1, USB_DEVICE); every other block publishes no grade table and was passed over
```

No `STRICT-GRADE` note in either trace. `documented` is the highest level
the registers on this path support: the detector reads UART0's
`rxd_cnt` on every loop — on **both** consoles — and the auto-baud counters
are the PAC's statement computed from the stated host baud, which no
transcript has measured.

## G1-7: what the ROM touched, and its grade

USB console path (run 5), UART0 path (run 4) — the union:

| block | register | grade | source |
|---|---|---|---|
| USB_DEVICE | `ep1`, `ep1_conf` | measured | `usb_sj.rs` header (M6 P4): `upload-walk-usb`, `boot-idle`, `usb-*` transcripts |
| UART0 | `fifo`, `status`, `int_st`, `int_ena`, `int_clr`, `clkdiv`, `conf0`, `conf1` | measured | `uart.rs` header: the UART0-link pairs `boot-idle` / `shader-compile-stress` at `d6cfaa205` and `upload-walk` |
| UART0 | `fsm_status`, `reg_update`, `rx_filt`, `rxd_cnt`, `lowpulse`, `highpulse`, `clk_conf` | documented | `uart.rs` header: the PAC's statement, implemented (auto-baud section; `clk_conf` bits 24/25 gate the shifter and receiver) |
| INTERRUPT_CORE0 | `core_0_intr_map43` | — | the interrupt-matrix model (M3); this block publishes no grade table |
| PLIC_MX | `mxint_enable`, `mxint_type`, `mxint5_pri`, `mxint_thresh` | — | same |

## Deviations and findings the brief did not predict

1. **The `.usb` script's stamps were milliseconds mistaken for microseconds.**
   The brief, notes F22 and M7's `_DONE` all say "SYNC at 20,000 µs"; the
   file said so in its comment; the grammar says ms. Fixed in the file (the
   bytes are unchanged), and the file is now a gate.
2. **The first SYNC on UART0 is never answered — by the ROM's design.** The
   UART0 script sends two. This is the ROM's behaviour, not the model's, and
   esptool's retry count exists for it.
3. **`clk_conf`'s `tx_sclk_en` / `rx_sclk_en` are now modelled** (a gate on
   the shifter and the receiver), because the ROM writes `clk_conf` on the
   console path and a register the model stores without acting on cannot
   honestly be graded above `modeled`. The PAC reset and the ROM's write both
   have the enables set, so no existing transcript moves.
4. **Grading the UART block puts UART0/UART1 into `--strict-grade`'s scope**,
   which changes the report line M6's gate
   `tests/usb_attached.rs::g4_4_the_shipped_image_crosses_no_modeled_usb_register`
   pins (`vec!["USB_DEVICE"]`). That file is fenced for this phase; see the
   phase report for the exact hunk.

## What the ROM-up ADR amendment (M3 P2) must say

`docs/adr/2026-09-06-esp-soc-emulator-architecture.md` §"ROM-up" currently
says the download console "does not answer commands". It should say: the
real ROM's download console answers on both consoles with no hook and no
change to the USB model — SYNC and `READ_REG` on USB from M6's `ep1` /
`ep1_conf`, and on UART0 through an auto-baud model (`periph/uart.rs`) whose
counters are computed from the host's stated baud (`--uart0-baud`, default
115,200) and the bytes the host actually sent; that the first SYNC on UART0
is spent on that detection, as on silicon; that the UART block now publishes
a per-register grade table (`measured` for what the UART0-link transcripts
exercised, `documented` for the auto-baud counters and `clk_conf`'s clock
enables, `modeled` for storage), so `--strict-grade` checks three blocks; and
that `scripts/rom-download-sync.usb` / `.uart0` are gates
(`tests/rom_download_console.rs`), not scripts run by hand.
