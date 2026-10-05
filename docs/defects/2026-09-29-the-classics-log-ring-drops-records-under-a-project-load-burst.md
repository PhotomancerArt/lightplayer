---
status: open   # reduced, not fixed, in the default build: the link thread that drained the ring during a load burst (M2 P4, 0 drops emulated) is opt-in since the 2026-10-03 silicon A/B; the default keeps P2's event-driven wake (14 drops emulated, against main's 25)
found: 2026-09-29      # how: emulator walk (lp-emu:esp32v3:t1), plan classic-uart-on-lp-link P5
area: fw-esp32-common `log_ring_logger` (4 KiB `LOG_RING`, `pump`) × `uart_link/uart_link_task.rs` (2 records per pass, thread executor)
class: wake-quantum-throttle
related:
  - ../adr/2026-09-27-lp-link-one-comms-layer.md
  - ../adr/2026-08-25-classic-uart-io-task-executor-isolation.md
  - ../adr/2026-10-02-c6-link-io-thread.md (2026-10-03 classic amendment)
  - 2026-08-02-serial-line-interleaving.md
  - lp2025/2026-09-28-2015-classic-uart-on-lp-link (plan dir, P5 §8, ruling DD37, P7)
  - lp2025/2026-10-02-1918-io-thread-other-boards (plan dir, P4)
---
# The classic's log ring drops records under a multi-output project-load burst

**Symptom** — on the classic on lp-link (wire proto 32), a project load
loses log records. Two shapes were seen:

- **Counted** by the ring, in-band:

  ```
  [LINK] 3 log records dropped
  [LINK] 21 log records dropped
  ```

  That is `lp-cli/tests/emu_v3_link_gates.rs`'s five-wire load
  (`five_wires_share_four_slots_…`, five outputs over four RMT slots):
  IO18's driver-open line is among them.
- **Not counted anywhere a host could see.** The oracle walk's open-time
  black `[OUT] dump` lost parts with no notice beside it (parts 2–5 in P5's
  run; parts 2, 3, 4 and 6 on `just walk-esp32v3-emu` at `b3142f3a7`). And
  when PR #886 ran the ported desk walk (`scripts/m4-hardware-walk.sh`)
  against this image hosted over a socket (`lp-emu-esp32v3 --uart0 tcp:…
  --reboot-on-reset`, host `lp-cli link capture --request reboot`) — the
  shape a desk sitting has — **no run of three got a whole lit dump**, the
  one thing the walk compares: parts 5/6, or 1, 5 and 6, missing. The host
  said `0 log record(s) lost on the wire`; the board's heartbeat said
  `"datagramsDropped":13`.

All `lp-emu:esp32v3:t1`; **not seen on silicon yet** (no desk sitting has
run this image).

**Root cause** — two mechanisms, measured apart on 2026-09-29 (P7) with a
scratch-instrumented link task (never committed):

1. **Fixed — the pump took records the link then refused.**
   `log_ring_logger::pump` popped a record off the ring and then offered it
   to `Link::send`. The classic's board link has a two-slot datagram queue,
   and a datagram leaves it only when the TX pipe has room for a whole
   frame, i.e. at UART line rate (921,600 baud, ~92 B per emulated ms). In
   a load burst the queue was still full at the next pass, `send` refused,
   and the popped record was gone: counted in the board's
   `datagramsDropped`, never given a datagram sequence number (so the
   host's gap count saw nothing), and never announced by the ring. On the
   socket-hosted reboot walk: 22 records refused in the first 0.5 s after
   the reboot, **every one after a pass that ended with the TX pipe full**
   (28 such passes), none while the link was stalled (0 stalls, 0 stalled
   passes holding records). Socket vs in-process is not the difference:
   the in-process twin of that shape (below) loses the same way,
   deterministically — `[(1, 4, 6), (31, 4, 6)]` (frame, parts arrived,
   of). What the socket-hosted walk has that `just walk-esp32v3-emu` does
   not is a **reboot into the saved project with the host already holding
   the link**: the boot, the auto-load, the compile and both dumps land in
   one burst (41 records in the ring at the link task's first pass, 33.6 ms
   after boot), where the in-process walk uploads after an idle boot and
   its lit dump comes after the backlog has cleared.
2. **Open — the ring overflows while nothing drains it.** The link task
   shares the thread executor with the engine (ruling DD20), and gets no
   pass during the boot's auto-load or a project load; and the UART drains
   slower than a load logs. The ring keeps the newest records and drops
   the oldest, **counted in-band** (`[LINK] n log records dropped`). After
   the reboot above: 19–21 records (socket), 32 (in process), all before
   the dumps. On the five-wire load: `[LINK] 4 …` and `[LINK] 21 …`, IO18's
   open line among them, **with or without fix 1** (measured both ways).

**Fix (1)** — `lp_link::Link::datagram_room` (free datagram slots), and
`pump` takes no more records than that: a record the link cannot queue now
waits in the ring. It cannot be lost silently any more; at worst the ring
drops it oldest-first and says so. No RAM, no flash to speak of; shared
with the C6's USB link task (same `pump`). The dumps it cost now arrive
whole: the socket-hosted walk 3/3 (`PASS: esp32 frame is byte-identical to
the host oracle (384 hex chars).`, `"datagramsDropped":0`), and both dumps
whole on `just walk-esp32v3-emu`.

**Why (2) is not fixed here** — measured, the five-wire burst needs a
larger ring, and on the classic `.stack`'s residual pays for every static.
A scratch image with an 8 KiB ring held the whole five-wire load (ring peak
**6,548 B**, 0 drops, IO18's open line arrived) at a cost of **4,096 B of
stack**: `main stack 33792 B`, and the five-wire load's high-water
**30,336 B — 3,456 B of headroom**. A 16 KiB ring (stack 25.6 KB) panicked
during the upload. A ring resize is a RAM trade for the director (plan
P7's brief), not a drive-by. A drain-rate change does not help much here:
the ring fills while the link task gets no pass at all, and even with a
pass the line cannot carry a load's log as fast as the load writes it.

**Regression coverage** —
- `lp-link/tests/link_scenarios.rs`
  `datagram_room_counts_the_slots_send_would_fill`, and
  `fw-esp32-common` `log_ring_logger`'s
  `a_record_waits_in_the_ring_while_the_link_has_no_slot_for_it` (fails on
  the old pump: 2 records refused).
- `lp-cli/tests/emu_v3_link_gates.rs`
  `a_reboot_into_the_saved_project_delivers_every_dump_whole` (in
  `just test-emu-esp32v3-boot`, so CI's classic job): the socket walk's
  shape in process — direct load, the merged chip image as the flash, a
  deploy, a `Reboot` over the link, and every `[OUT] dump` after it whole,
  the lit one the oracle's frame. The pre-fix image fails it; the fixed one
  passes.
- The socket-hosted shape itself runs on host wall-clock time, so it is a
  recipe, not a CI gate (once PR #886's tooling is on `main`):

  ```
  lp-emu-esp32v3 --merged <frame-dump merged.bin> --uart0 tcp:127.0.0.1:5592 \
      --reboot-on-reset --timeout 400s &
  LP_CLI=target/release/lp-cli scripts/m4-hardware-walk.sh --chip esp32 tcp://127.0.0.1:5592
  ```

  (`just build-fw-esp32v3 frame-dump`, then
  `scripts/emu/build-merged-image.sh --chip esp32 <elf> <merged.bin>`.)
- The five-wire gate still accepts IO18's open line **or** the ring's drop
  notice, and now prints which arrived.

**Update 2026-10-03 (M2 P4)** — root cause 2 ("the ring overflows while
nothing drains it") is a direct consequence of the link task sharing the
render's thread executor: it gets no pass while the engine holds the
executor for a load burst, which is exactly when a project's logs are
loudest. Plan `lp2025/2026-10-02-1918-io-thread-other-boards` moves the
link task (and its log pump) to its own priority-1 esp-rtos thread, pinned
to core 0 (`fw-esp32v3/src/io_thread.rs`, feature `io-thread`, default on;
`docs/adr/2026-10-02-c6-link-io-thread.md`'s classic amendment). A thread
of its own gets scheduled even while the render holds its own thread, so
the ring is pumped during a load burst instead of only after it. Measured
on `lp-cli/tests/emu_v3_link_gates.rs`'s five-wire load
(`lp-emu:esp32v3:t1`, lp-emu `ab8345d38`): drops fell from main's 25 (P2's
event-driven-wake-only arrangement: 14) to **0** — IO18's open line
arrives. The reboot gate's **second boot** still drops 32/32/33 records
(main/P2/P4) on all three arrangements: those are logged during the reboot
itself, before any host has the link back up to drain them, which no
thread placement can fix (there is no reader yet) — a separate, narrower
gap than the one this entry tracks, not reopened here.

This closes the entry **on the emulator**; root cause 1 (the datagram-room
fix) was already closed on 2026-09-29. Status is `fixed (emulated; silicon
at the M2 desk walk)` because no desk sitting has run this arrangement yet
(`desk-classic.md`, batched with #884's owed classic lp-link walk) — a
silicon disagreement with this emulated result would be a fidelity defect
to file before reopening this one.

**Update 2026-10-03 (silicon A/B) — reopened.** The desk A/B on the
DOM-Z-102 found the link thread slower and less even than the main-executor
arrangement in every setting (a preemption costs this chip's render ~4–5 ms
of flash-cache refill), so `io-thread` is **opt-in, off by default**
(`docs/adr/2026-10-02-c6-link-io-thread.md`, classic amendment, "Silicon
(2026-10-03)"). The default build therefore pumps the ring only between
frames again, with P2's event-driven wake: 14 drops on the five-wire load
emulated, against main's 25 and the thread's 0. The desk sitting itself saw
main drop 4 records in a five-wire load burst on silicon. Root cause 2 is
open again; a fix that does not preempt the render (a bigger ring, fewer
records per load, or pumping from the render's own yield points) is the
next step.

**Lesson** — a best-effort queue that refuses is only honest if nothing was
taken to offer it: popping from one bounded buffer into another that can
say no turns a counted overflow into a silent one. Ask for room first. And a
log path sized for one chip's drain rate (the C6's USB link task) moved
unchanged onto a chip whose line is ~10x slower and whose link task the
engine can hold off; the burst that exposed it is exactly the moment the
logs are most wanted: a project coming up.

**Silicon, 2026-10-03 (agent-run desk sitting, DOM-Z-102)** — the emulated
result holds on the board. Uploading `projects/test/five-wire` (which resets
the board through the CH340 and then loads it), main `d68791d96` reported
`[LINK] 16`/`19 log records dropped` at boot, before the link came up, and
`[LINK] 4 log records dropped` in the load burst, both uploads alike; PR
#943's `96dfc8cca` reported **none, of either kind**, over two uploads (and
none in any of its six other boots). The silicon burst is smaller than the
emulator's (4 vs 25 records): the board's frame is ~14× longer in real time
than the emulated one, which plausibly spreads the burst over more link passes (not measured)
(`2026-10-03-the-emulated-classic-renders-14x-faster-than-silicon-…`).
