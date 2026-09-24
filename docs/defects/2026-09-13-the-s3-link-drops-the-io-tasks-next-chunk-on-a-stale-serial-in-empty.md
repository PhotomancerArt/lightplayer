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

**2026-09-23 — the boot hello no longer shows it; the mechanism is
unchanged.** On `feat/lp-json-pack` (PR #795: a token hook in the vendored
`ser-write-json`, struct-field prefix helpers moved `#[inline(never)]`,
base64 blobs streamed through `collect_str`) the shipped image delivers the
whole `hello` on both boot paths — direct `1586 bytes reached the host; 0
bytes were merely tried`, ROM-up `3732 … 0` — where `origin/main`
(a9429460f), run by the same emulator binary, still reads `1524 … 64` and
`3670 … 64` with the packet above on the tried stream. The `USB_DEVICE`
trace (direct, 300 ms, `ALIAS`/byte lines removed) shows why. The io_task's
first poll clears `serial_in_empty` as it takes the link, and it is a race
against the drain of the `[INIT]` chain's last 1-byte packet:

```
origin/main:
cyc=4396066 pc=0x4208d5e0 W4 USB_DEVICE+0x014 int_clr = 0x00000008   (io_task poll)
cyc=4396206 pc=0x4208d6b2 USB_DEVICE IN packet of 1 bytes delivered … serial_in_empty raised
cyc=28398267 pc=0x4208d7eb R4 USB_DEVICE+0x008 int_raw = 0x0000030a   (stale bit 3)
feat/lp-json-pack:
cyc=4396360 pc=0x420fb8c5 USB_DEVICE IN packet of 1 bytes delivered … serial_in_empty raised
cyc=4400882 pc=0x4208d488 W4 USB_DEVICE+0x014 int_clr = 0x00000008   (io_task poll)
cyc=28403083 pc=0x4208d693 R4 USB_DEVICE+0x008 int_raw = 0x00000302   (bit 3 clear)
```

On main the clear lands 140 cycles *before* the drain re-raises the bit, so
the bit is stale when the hello's write future arms. On the branch the main
task is still inside `ser_write_json::ser::format_escaped_str_contents` when
the packet drains (the slower JSON path delays the io_task's first poll),
the clear lands 4,522 cycles *after* the drain, and the hello's first chunk
waits the full 100 µs (`int_st` at 28,428,827, 24,291 cycles after its
`int_ena = 0x08`). Nothing in esp-hal, esp-println or the link model
changed: this is timing, and the defect is **latent** at the hello. Its
live witness is the stop-all reply — `the_ledger_triple_is_elicited_by_a_stop_all_on_the_wire`
still reads it on the tried stream (37 bytes) on this branch. The hello
pins in `tests/boot_idle.rs` and `tests/boot.rs` now assert the hello
**delivered whole** (0 tried, the once-dropped packet present), so a
timing shift that re-opens the race fails by name. Emulator:
`lp-emu-esp32s3` built from this worktree at d5f29f337 (t1).

**Lesson** — an interrupt raw bit is state, and a driver that arms an
enable without clearing the raw first is asserting "nothing has happened
since I last looked" — which is only true for the code path that cleared
it last. Two writers on one FIFO (a polled printer and an interrupt-driven
transport) break that assumption without either being wrong on its own.
For the emulator: a `measured` grade names the path it was measured
through; a second writer on the same register is a second configuration,
and the model's answer to it is `modeled` until a board says otherwise.
