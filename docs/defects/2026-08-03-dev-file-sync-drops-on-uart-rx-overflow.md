---
status: open
found: 2026-08-03      # how: hardware-walk
area: lp-cli/src/commands/dev (fs sync) + fw-esp32v3 UART0 RX
class: silent-drop
related:
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

**Regression coverage** — none yet; not reproduced outside the walk, and
the RX-overflow hypothesis has no isolated repro.

**Lesson** — the classic/S3-family chip's single shared UART (no
USB-Serial-JTAG separating host link from logs) keeps surfacing as a
one-resource-many-writers problem:
`docs/defects/2026-08-02-serial-line-interleaving.md` found it corrupting
outbound telemetry lines; this is the same wire's inbound side apparently
losing data under the same kind of load. Two independent findings against
one shared, unowned UART in the same week is the debt-register filing bar
(`docs/debt/README.md`) starting to be met, not two unrelated bugs — worth
naming as a structural burden if a third instance turns up.
