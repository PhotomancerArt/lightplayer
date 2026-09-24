---
status: open
found: 2026-09-13      # live-debugging (M6 P06 of lp2025/2026-09-10-0021-xtensa-emulator)
area: lp-emu/esp/lp-emu-esp-common/src/ip/usb_sj.rs (the USB-Serial-JTAG link model) × esp-hal 1.1.1 usb_serial_jtag::write_async × fw-esp32s3 serial/io_task
class: backend-contract-divergence
related:
  - lp2025/2026-09-10-0021-xtensa-emulator/m6/p06-flash-cache-and-rom-up.md
  - lp2025/2026-09-10-0021-xtensa-emulator/m6/p05-the-link-and-the-hello.md
  - lp-emu/esp/lp-emu-esp32s3/tests/boot_idle.rs
---
# The S3 link drops the io_task's next chunk because esp-hal's write future wakes on a stale `serial_in_empty`

**Symptom** — on the emulated ESP32-S3, both boot paths, with a draining
host attached from power-on, the server loop's first framed write — the
unsolicited `hello`, ~1 KB — reaches the host **missing one 64-byte packet**
from inside its feature list, and the run report says so:

```
usb-sj: host attached at power-on; 1524 bytes reached the host; 64 bytes were merely tried
```

The tried stream holds exactly the packet:
`.button","node.clock","node.fluid","node.fixture","node.playlist`. A
`stopAllProjects` sent over the wire is handled (the ledger triple prints,
`Stopped all projects` is logged) but its **reply**
`M!{"id":1,"msg":"stopAllProjects"}` lands on the tried stream too. The
mask ROM's USB copy of its banner loses whole lines the same way
(`oad:0x3fce3818,len:0x16f8` — `Build:`, `rst:`, `SPIWP:`, `mode:` gone);
UART0 carries that banner whole.

**Root cause** — three parties, one register. The `USB_DEVICE` trace on the
direct path, with the `ALIAS` and byte-write lines removed:

```
cyc=4396206  pc=0x4208d3ae USB_DEVICE IN packet of 1 bytes delivered to the host (serial_in_ep_data_free = 1, serial_in_empty raised)
cyc=28398267 pc=0x4208d4e7 R4 USB_DEVICE+0x008 int_raw = 0x0000030a
cyc=28398271 pc=0x4208d4f2 W4 USB_DEVICE+0x014 int_clr = 0x00000002
cyc=28399703 pc=0x4207bb73 USB_DEVICE wr_done: 64 bytes committed to the IN endpoint; the host takes them in 100 us
cyc=28399720 pc=0x4207bb9e W4 USB_DEVICE+0x010 int_ena = 0x00000008
cyc=28400045 pc=0x420a556d R4 USB_DEVICE+0x00c int_st = 0x00000008
cyc=28400062 pc=0x420a5599 W4 USB_DEVICE+0x014 int_clr = 0x0000000c
cyc=28400254 pc=0x4207bb73 USB_DEVICE ep1 write with the IN FIFO committed (host attached): byte 0x2e dropped
```

1. **esp-println** (`0x4208d3ae`, `esp_println::Printer::write_bytes`)
   writes the `[INIT]` chain by polling `serial_in_ep_data_free`; when the
   host drains its last packet the model raises `serial_in_empty` in
   `int_raw` (bit 3). Nobody is listening (`int_ena = 0`) and nobody
   clears it — the io_task's later `int_clr = 0x02` (at `0x4208d4f2`,
   its RX path) clears only `serial_out_recv_pkt`. The raw bit is now
   **stale**.
2. **esp-hal**'s `write_async` (`usb_serial_jtag.rs`, inlined into
   `ChunkedWriter::try_write_all_with` at `0x4207bb73`) writes 64 bytes,
   sets `wr_done`, and awaits `UsbSerialJtagWriteFuture::new()`, which
   only *sets* `int_ena.serial_in_empty` — it never clears the raw bit
   first. `int_st = int_raw & int_ena` is 1 at once, the async handler
   (`0x420a556d`) fires 342 cycles after the commit, clears the enable and
   the raw, wakes the future, and the next chunk is written.
3. **The link model** holds the committed packet for
   `IN_DRAIN_LATENCY_US = 100` (24,000 cycles at t1) and, per its own
   contract, drops a byte written into a committed FIFO. All 64 bytes of
   the second chunk go to the tried stream; esp-hal's `wr_done` on an
   empty FIFO then raises `serial_in_empty` at once, the third chunk waits
   correctly, and the rest of the message is fine.

The reply's loss is the same shape one packet earlier: the triple's last
`esp_println` packet commits, and the io_task's framed reply write follows
inside the drain latency.

**Which side is wrong is not yet known**, and that is why this is open
rather than fixed here:

- If **silicon** also refuses a write while the IN packet is pending, then
  esp-hal's future is wrong on hardware too and the same bytes vanish on
  the M4-walk S3 board — testable with `lp-cli`'s transcript of one long
  `hello` on the board (P09's capture is the natural place).
- If **silicon accepts** it (the `USB_DEVICE.in_ep1_st` write pointer is
  seven bits wide on this part, room for two 64-byte packets), the link
  model's single committed FIFO is the divergence, and its
  `measured`-graded claim "`serial_in_ep_data_free` returns only once a
  host has drained the packet" was measured through esp-println's polled
  path alone, which never writes into a pending endpoint.
- Either way esp-hal's `UsbSerialJtagWriteFuture::new` arming on a stale
  raw is a latent bug worth an upstream note, and the firmware could clear
  `serial_in_empty` before its first framed write regardless.

The C6 shares this model (`lp-emu-esp-common/src/ip/usb_sj.rs` since M6
P05) and the same esp-hal driver; whether its walk shows the same drop is
a question for its transcripts, not assumed here.

**2026-09-23 — the hello stopped showing it; the defect did not go
away.** On the BLE plan's M3 image (PR #794) the `hello` reaches the host
whole on both boot paths (`0 bytes were merely tried`, direct and ROM-up),
while `origin/main`'s image (`e226fb283`) still drops the packet. The
hello growing an `auth` field is **not** the cause. The raw bit only goes
stale if the host drains esp-println's last `[INIT]` packet *after* the
io_task's first poll has run its driver set-up, which writes `int_clr =
0x08` (`serial_in_empty`) — a clear the root cause above does not
mention, and it is what decides the race. The last polled packet commits
and is drained `IN_DRAIN_LATENCY_US` = 24,000 cycles later:

| image | last `[INIT]` packet commits | io_task clears `serial_in_empty` | host drains it | raw at the hello |
|---|---:|---:|---:|---|
| `e226fb283` (main) | 4,372,295 | 4,396,155 (+23,860) | 4,396,295 (+24,000) | set → stale: `int_raw = 0x30a`, packet dropped |
| PR #794 | 4,378,368 | 4,404,669 (+26,301) | 4,402,368 (+24,000) | clear: `int_raw = 0x302`, nothing dropped |
| PR #794 merged over lean-wire (`ebf63d463`), re-measured the same day, clean build | 4,348,791 | 4,375,134 (+26,343) | 4,372,791 (+24,000) | clear: nothing dropped (`1621 bytes reached the host; 0 bytes were merely tried`) |

On main the clear won by **140 cycles**; the branch spends 2,441 more
cycles between the last polled packet's commit and the io_task's first
poll (not attributed further; the branch changed the server loop that
runs on that main task), so the drain lands
first and the clear wipes it. Read off `--trace-block USB_DEVICE` of both
images, direct load, `--usb-host attached`, 2 s.

Lean-wire (PR #791) moved the whole prefix ~30k cycles earlier but left the
gap where it was (+26,343 against +26,301), so the merged image stays on the
no-drop side by 2,343 cycles and the three hello pins hold at 0 unchanged. Same
trace, the merged tree's image built from a clean checkout (a dirty
build's image differs — `LP_BUILD_DIRTY` is in it — and moved these figures
by a few dozen cycles; CI builds clean).

So the hello trigger is a timing race with a 100 µs window, and which side
of it an image falls is layout, not design. **The mechanism is unchanged
and still reproduces** on the second trigger: a stop-all's reply, written
straight after the triple's last `esp_println` packet, is still on the
tried stream on the branch's image
(`tests/boot_idle.rs::the_ledger_triple_is_elicited_by_a_stop_all_on_the_wire`,
passing unmodified). The three hello-path assertions
(`the_shipped_image_prints_its_init_chain_out_of_the_link`,
`a_usb_script_resolves_its_walk_forms_and_two_runs_are_the_same_run`,
`tests/boot.rs::the_shipped_image_gets_past_esp_hal_init_and_crosses_the_console`)
are re-pinned to **0 tried**, pointing here; a future image that falls back
across the 140-cycle edge flips them back to 64, and that is this entry,
not a new finding.

**Fix** — none yet. Not P06's: the phase's scope is the flash, the cache
window, SHA and the ROM-up boot, and the link belongs to P05's model in
the shared crate.

**Regression coverage** —
`lp-emu/esp/lp-emu-esp32s3/tests/boot_idle.rs` pins the behaviour where it
shows, with this entry named beside each assertion, so that the fix flips
them rather than a reader's memory:
`the_shipped_image_prints_its_init_chain_out_of_the_link` (exactly one
64-byte packet tried, from the hello, absent from the delivered stream),
`the_ledger_triple_is_elicited_by_a_stop_all_on_the_wire` (the reply on the
tried stream and not the delivered one), and the host-absent /
port-closed / scripted variants; `tests/boot.rs::
the_shipped_image_gets_past_esp_hal_init_and_crosses_the_console` pins the
same 64 bytes at 300 ms.

**Lesson** — an interrupt raw bit is state, and a driver that arms an
enable without clearing the raw first is asserting "nothing has happened
since I last looked" — which is only true for the code path that cleared
it last. Two writers on one FIFO (a polled printer and an interrupt-driven
transport) break that assumption without either being wrong on its own.
For the emulator: a `measured` grade names the path it was measured
through; a second writer on the same register is a second configuration,
and the model's answer to it is `modeled` until a board says otherwise.
