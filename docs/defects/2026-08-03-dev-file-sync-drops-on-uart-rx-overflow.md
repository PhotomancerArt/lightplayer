---
status: fixed
found: 2026-08-03      # how: hardware-walk
fixed: d06d0928b      # structurally, by lp-link's ARQ (PR #884, wire proto 32) — emulator-validated, desk walk pending
area: lp-cli/src/commands/dev (fs sync) + fw-esp32v3 UART0 RX
class: silent-drop
related:
  - ../adr/2026-09-27-lp-link-one-comms-layer.md
  - ../debt/shared-uart-io-task-starvation.md
  - 2026-08-02-serial-line-interleaving.md
  - 2026-08-03-0903-multi-endpoint-output-node (plan dir, P6 hardware walk)
---
# `lp-cli dev` file-sync writes vanish on UART RX FifoOverflow under a flooding TX

**Symptom** — During the P6 hardware walk (`projects/test/quad-wire-oracle`,
desk DOM-Z-102, frame-dump build), file changes pushed through `lp-cli dev`'s
watch-and-sync loop (`lp-cli/src/commands/dev/sync.rs::sync_file_change`)
were observed to silently not take effect on the device: no error surfaced
to the CLI, the sync appeared to proceed normally, and the project on device
did not reflect the change. `lp-cli upload`'s one-shot push of the same
files, over the same serial link and around the same conditions, did not
exhibit the drop.

**Root cause (suspected, not yet investigated)** — The frame-dump build's
`[OUT]`/`[MEM]` telemetry lines flood UART0 TX continuously — the same
concurrent-writer population documented in
`docs/defects/2026-08-02-serial-line-interleaving.md`. `dev`'s sync writes
travel inbound (RX) on that same wire; the working theory is the device's
UART RX FIFO overflows while the device is busy servicing the TX flood, so
inbound bytes are silently dropped before `lp-cli`'s write request ever
reaches the server-side handler. `upload` not exhibiting the same drop is
consistent with it sending its payload in fewer, larger frames that may not
have landed inside a flooding window, but that is unconfirmed — this entry
records an observation from a walk whose primary goal was the multi-channel
output verification (P6), not a transport investigation.

**Why it matters** — `lp-cli dev` is the primary shader-authoring loop
(edit → save → watch pushes it automatically). A silently dropped push has
the same shape as `docs/defects/2026-07-30-deploy-compiles-previous-upload.md`'s
lesson: an author who does not see their edit take effect suspects their own
change before suspecting the transport, and on firmware built specifically
to flood the serial link with diagnostics (a frame-dump build), that
suspicion lands in exactly the wrong place.

**Not yet done** — no root-cause investigation or fix attempted. Candidate
directions, unevaluated: give `sync_file_change` (or the wire round-trip
under it) an explicit response timeout with a surfaced error instead of
succeeding silently; reduce or gate frame-dump's UART volume while a `dev`
session is attached; move file-sync onto whatever larger-frame, less
interleaved write pattern lets `upload` survive the same conditions.

**Root cause CONFIRMED — 2026-08-21 (serial-lab, dig2go bench)** — The
suspected mechanism is real but the driver is the ENGINE TICK, not a TX
flood: the io_task shares a cooperative executor with the server tick
(~41 ms under a playing project), the RX FIFO is 128 B (~1.4 ms at
921600), so any inbound burst landing inside a tick overflows —
`UART RX error: FifoOverflowed; dropping partial line` — even paced at
256 B / 25 ms. With projects stopped (tick = 0 ms) a 4.5 KB frame lands
instantly. The outbound twin also confirmed: protocol responses are
DROPPED on `[io_task] UART TX timed out` under the same contention
(`responses=0` on every perf line), which is why `upload`'s run-evidence
wait times out against a playing device. `upload` surviving in the
2026-08-03 observation is now explained: one-shot uploads run against a
freshly-flashed device with no project ticking. Structural home:
`docs/debt/shared-uart-io-task-starvation.md` (this was the third
instance; the debt bar this entry's lesson set has been met). Repro and
instrumentation live in `spikes/serial-lab/`.

**Regression coverage** — none yet; the debt entry's exit criteria name
the test (≥4 KB inbound frame while a dome-scale project ticks).

**Lesson** — the classic/S3-family chip's single shared UART (no
USB-Serial-JTAG separating host link from logs) keeps surfacing as a
one-resource-many-writers problem:
`docs/defects/2026-08-02-serial-line-interleaving.md` found it corrupting
outbound telemetry lines; this is the same wire's inbound side apparently
losing data under the same kind of load. Two independent findings against
one shared, unowned UART in the same week is the debt-register filing bar
(`docs/debt/README.md`) starting to be met, not two unrelated bugs — worth
naming as a structural burden if a third instance turns up.

**Fixed structurally — 2026-09-29, by lp-link's ARQ (PR #884, wire proto
32).** Precisely what changed, and what did not:

- **The FIFO can still overflow.** UART0's RX FIFO is still 128 B, there is
  still no flow control, and nothing in this change makes an inbound byte
  impossible to lose on the line. (PR #448's executor isolation had
  already made the 2026-08-21 mechanism — a whole engine tick with no RX
  service — rare: io_task drains the FIFO every 1 ms.)
- **A lost byte is no longer silent, and no longer fatal.** Every link frame
  carries a CRC-32C; a frame that lost or gained a byte fails it, is counted
  (`damaged` on the board's end, surfaced in the heartbeat's `link`
  object), and is resent by the host until the board acknowledges it
  (selective repeat). A write request is delivered to the server whole or
  not at all, and "not at all" ends in a link reset the host sees, which
  fails the request at once — never a success with the file missing.
- **The outbound twin** (responses dropped on `UART TX timed out`) is
  covered by the same mechanism in the other direction: a reply that does
  not get out is resent by the board.

Evidence, emulated only (`lp-emu:esp32v3:t1`,
`lp-cli/tests/emu_uart_link.rs`): a `--uart-faults` soak that drops, cuts
and corrupts 64-byte windows of UART0's byte stream in both directions — up
to ~2.5 % of windows, and 1 KiB runs — completes five project uploads and
loads with **0 app errors**, each end counting the damage that reached it.
P2's first upload of `zook-dome` over the emulated UART saw one real RX
FIFO error on the board and it surfaced as `resends 1, damaged 1`, with the
project loaded. The desk walk (`hardware-walk-protocol.md` in the plan dir)
reads the same counters on silicon: a non-zero `resends` there is this
defect's old loss, now recovered and visible.

**Regression coverage** — the fault soak above (`emu_uart_link.rs`, run by
`just test-emu-esp32v3-boot` in CI's `Emulator ESP32v3 (x64)` job).
