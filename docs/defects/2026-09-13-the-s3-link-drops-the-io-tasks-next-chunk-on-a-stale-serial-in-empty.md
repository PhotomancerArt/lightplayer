---
status: fixed
found: 2026-09-13      # live-debugging (M6 P06 of lp2025/2026-09-10-0021-xtensa-emulator)
area: fw-esp32s3 serial/io_task (and, latent, fw-esp32c6's) × esp-hal 1.1.1 usb_serial_jtag::write_async (the link model lp-emu/esp/lp-emu-esp-common/src/ip/usb_sj.rs was right)
fixed: this change     # firmware-side gate in fw-esp32s3 serial/in_endpoint.rs (PR #805); the C6 is a follow-up, see the 2026-09-24 note
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
rather than fixed here (*settled 2026-09-24 from the documents: silicon
refuses; see the dated note below*):

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

**Fix** — (2026-09-24) in the firmware, not the model and not esp-hal:
`serial::in_endpoint::InEndpoint` in `fw-esp32s3` wraps
esp-hal's async TX half and, before every io_task write, waits until
`serial_in_ep_data_free` reads 1 (clear the raw `serial_in_empty`,
recheck, else esp-hal's `flush`, which arms on a bit now only a real drain
can raise) and then clears the — by then stale — raw bit, so the one thing
that completes esp-hal's write future is the drain of the packet this
write commits. Nothing in esp-hal is patched. Cost: +480 B `.text` and
+24 B `.bss` on the S3 image. The C6 is **not** changed here (below).

**Regression coverage** — (2026-09-24, the mechanism rather than a byte
count) the link model's own tests in `lp-emu-esp-common`
`ip/usb_sj.rs` —
`a_write_into_a_pending_packet_is_refused_and_lands_on_the_tried_stream`,
`a_stale_serial_in_empty_wakes_esp_hals_write_future_at_once_and_the_next_chunk_is_refused`
and `the_in_endpoint_gate_delivers_every_byte_on_both_shapes` — pin the
contract and both failure shapes at the registers, independent of any
image's timing. The S3 machine grew `Machine::usb_sj_refused()` (bytes
written into a pending or full buffer), and its `tests/boot_idle.rs`
and `tests/boot.rs` assert it is 0 on every host path — draining, absent,
closed-then-opened, scripted, and the stop-all whose reply is now
delivered. What follows is the pre-fix coverage, kept for the history:
`lp-emu/esp/lp-emu-esp32s3/tests/boot_idle.rs` pinned the behaviour where it
shows, with this entry named beside each assertion, so that the fix flips
them rather than a reader's memory:
`the_shipped_image_prints_its_init_chain_out_of_the_link` (exactly one
64-byte packet tried, from the hello, absent from the delivered stream),
`the_ledger_triple_is_elicited_by_a_stop_all_on_the_wire` (the reply on the
tried stream and not the delivered one), and the host-absent /
port-closed / scripted variants; `tests/boot.rs::
the_shipped_image_gets_past_esp_hal_init_and_crosses_the_console` pins the
same 64 bytes at 300 ms.

**2026-09-24 — the documents settle it: silicon refuses, the model was
right, and the fix is the firmware's.**

*The question* was whether the EP1 IN buffer accepts a write while a
previous packet is pending (committed with `wr_done`, not yet taken by the
host). What the sources say:

- **ESP32-C3 TRM v1.3, §30.3.2 "CDC-ACM Firmware Interface Functional
  Description", p. 767** (read from the PDF; the same USB-Serial-JTAG IP
  block the C6 and S3 carry): the send buffer is filled while
  `SERIAL_IN_EP_DATA_FREE` is 1; a flush happens on the 64th byte or on
  `WR_DONE`; after a flush of either kind the buffer is "unavailable for
  firmware to write into" until the host has read all of it, and only then
  does `SERIAL_IN_EMPTY_INT` fire to say another 64 bytes fit. One buffer,
  refused while pending. The register chapter (Register 30.6
  `USB_SERIAL_JTAG_EP1_CONF_REG`, p. 777) gives `WR_DONE` and
  `SERIAL_IN_EP_DATA_FREE` the same reading.
- **The C6 and S3 PACs** (`esp32c6-0.23.2`, `esp32s3-0.35.2`, generated from
  Espressif's SVDs): `SERIAL_IN_EP_DATA_FREE` is documented, word for word
  alike on both chips, as 0 from `WR_DONE` until the host has read the
  data; `EP1.RDWR_BYTE` says to write "up to 64 bytes" when
  `SERIAL_IN_EMPTY_INT` is set.
- **ESP-IDF's LL driver** (`components/esp_hal_usb/esp32s3/include/hal/usb_serial_jtag_ll.h`,
  Apache-2.0, esp-idf master `188e3e55b`):
  `usb_serial_jtag_ll_write_txfifo` checks `serial_in_ep_data_free` before
  **every** byte and stops when it reads 0 — the vendor's own writer never
  writes a pending buffer.
- **The seven-bit `in_ep1_st` address is not a second buffer.** In the C3
  TRM's Register 30.9/30.10 (p. 779) every IN endpoint's status register —
  `IN_EP0_ST` for the 64-byte control endpoint included — has the same
  7-bit `WR_ADDR`/`RD_ADDR` pair, and a count of 0…64 needs seven bits.
  Suggestive was all it ever was.
- **Silicon corroboration, from upstream.** esp-hal PR #6104 (merged
  2026-08-12, `ab45d33cf6`, shipped in esp-hal **1.2.0**), "prevent async
  data loss under bidirectional load", reproduced truncated frames on a
  physical ESP32-C3 and traced half of them to exactly this: a completed
  write future "treated as sufficient proof that the TX FIFO was writable"
  when an early interrupt had woken it. Their fix re-checks
  `SERIAL_IN_EP_DATA_FREE` after the wake. That is one of our two shapes,
  on silicon, with the loss observed.

⚠️ The C6 and S3 TRMs themselves were **not** read this session: both PDFs
exceed the fetch tool's 10 MB limit and downloading them needs Yona's say.
The C3 TRM is the same IP and the C6/S3 PAC text agrees with it word for
word; if anybody wants the citation on the chips' own manuals, it is the
"CDC-ACM Firmware Interface Functional Description" subsection of each
USB Serial/JTAG chapter. **What the documents do not say** is what
silicon does with a byte written anyway (discarded, or overwriting the
pending packet): the model discards it onto the tried stream, graded
`modeled`. The fix below makes the firmware never find out.

*So the model is right and esp-hal 1.1.1 is wrong twice:*
`write_async` pushes a chunk without reading `serial_in_ep_data_free` (the
stop-all reply's shape — esp-println's packet still pending), and its
write future arms `serial_in_empty` without clearing a stale raw (the
hello's shape — woken before its own packet drains, so the next chunk goes
into a pending buffer). esp-hal 1.2.0 fixes the second (#6104's re-check)
and **not the first**; upstream `main` at `5edb7b89c` still writes the
first chunk of every `write_async` without a free check.

*The fix* is the firmware gate above, in the S3's io_task. Emulator runs
(`lp-emu-esp32s3` built from this branch, `t1`): the shipped image
delivers the whole hello with `0 bytes merely tried` and **0 refused** on
both boot paths, and a stop-all's reply `M!{"id":1,"msg":"stopAllProjects"}`
now reaches the host; with no host or a closed port the io_task waits
instead of writing into the held packet (host-absent tried 22 B —
esp-println's one line — where it was 22 + 64 + 2). The S3 image's `.bss`
grew 24 B, so its stack total is 37,256 B (was 37,280 on main after
#804; 37,272 against 37,296 before that merged), re-baselined in
`scripts/heap-budget-record.json`.

*The C6 is latent, and deliberately left for its own change.* It has the
same driver and the same second writer (esp-println carries its `[INIT]`
chain, panics, and the watchdog and stress lines), but no C6 run has lost
a protocol byte to it (only its `\n` probes into a held packet, which
are harmless): its logs ride the io_task, so a stop-all's reply does not
follow an esp-println packet (checked on this branch: stop-all on the shipped C6
image, 0 bytes tried, the reply delivered). The same gate was built and
run against the C6 (+272 B image, headroom 714,576 B) and it changes the
C6's measured not-draining signature: with the port closed or no host, the
io_task no longer writes its first chunk and its `\n` probes into the held
packet, so `usb_attached.rs::g2_3_…`, `usb_control.rs::g3_1b_…` and
`host_absent.rs` — whose assertions are the M6 transcripts' shape, recorded
against silicon (`usb-negative-control`) — would all move. That is a
change to a silicon-graded behaviour, and it wants its own PR and a desk
re-check of the negative control, not a rider on this one.

*What a board would add.* Nothing is needed to close this — the refusal is
documented and the fix removes the firmware's dependence on what happens
past it. P09's S3 desk sitting (the hello and a stop-all reply captured
byte for byte) is the natural confirmation that the gated image delivers
both whole on silicon; record it there, not as a precondition here.

*Upstream (drafted, not filed — Yona files upstream):*

> **usb_serial_jtag: `write_async` writes the first chunk without checking
> `SERIAL_IN_EP_DATA_FREE`**
>
> esp-hal 1.2.0 / main `5edb7b89c`, `esp-hal/src/usb/usb_serial_jtag.rs`.
> `UsbSerialJtagTx::write_async` pushes each 64-byte chunk into EP1 and
> sets `WR_DONE`, then waits (`wait_tx_ready`) for the FIFO to be free
> again. The wait after each chunk is correct since #6104, but nothing
> checks that the FIFO is free **before the first chunk**. The driver
> assumes it is the endpoint's only writer; when another writer shares the
> endpoint — `esp-println` with the `jtag-serial` feature is the common
> case, and a blocking `UsbSerialJtag` or `esp-backtrace` are others — a
> `write_async` that starts while that writer's last packet is still
> pending writes into a buffer the TRM says is unavailable until the host
> reads it (ESP32-C3 TRM §30.3.2), and those bytes are lost.
>
> Suggested fix: call `wait_tx_ready()` (or an equivalent check of
> `SERIAL_IN_EP_DATA_FREE` that waits on `SERIAL_IN_EMPTY` only when it
> reads 0) at the top of each chunk, and clear the `SERIAL_IN_EMPTY` raw
> bit before `WR_DONE` so the wake that follows is this packet's drain
> rather than a latch left by the other writer. We work around it with a
> wrapper that does both before delegating to `write_async`.

*Follow-up left open*: the S3 walk's workarounds for this defect
(`scripts/emu/m4-walk.sh`'s `--no-wait` and lit-frame evidence,
`walks/shader-oracle.script` waiting on `Stopped all projects` instead of
the reply's `"id":1,`, and the `the_walk_is_the_c6s_captured_bytes` test's
one tolerated line) can now be re-pointed at the plain calls; that change
wants a run of `just walk-esp32s3-emu` to prove it, and is not in this PR.
Upgrading to esp-hal 1.2.0 would make half of the gate redundant; the other
half stays until upstream checks the first chunk.

**Lesson** — an interrupt raw bit is state, and a driver that arms an
enable without clearing the raw first is asserting "nothing has happened
since I last looked" — which is only true for the code path that cleared
it last. Two writers on one FIFO (a polled printer and an interrupt-driven
transport) break that assumption without either being wrong on its own.
For the emulator: a `measured` grade names the path it was measured
through; a second writer on the same register is a second configuration,
and the model's answer to it is `modeled` until a board says otherwise.
