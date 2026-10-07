---
status: fixed
found: 2026-10-07      # how: desk run (fixture C6, Mac Chrome via CDP, OTA M7 BLE speed pass)
fixed: ed15e1091
area: lpa-update `BackupSession` / `ServeConfig::BLE` (the read-back's size) × fw-esp32-common `LinkMuxTransport::release_radio_holders` (the one shared frame buffer)
class: lock-held-across-foreign-latency
related:
  - docs/defects/2026-10-06-a-stalled-radio-link-holds-the-tick-past-the-watchdog.md (the per-tick wait budget this meets)
  - docs/defects/2026-10-07-a-reconnected-link-refuses-the-backup-before-its-login-lands.md (what the reconnect then met)
  - docs/adr/2026-09-24-ble-transport.md (decision 3, the shared frame buffer)
  - lp2025/2026-10-05-0820-ota-studio-ble-updates (ble-speed-progress.md, surprise 1; ble-backup-fix.md)
---
# A Bluetooth backup holds the board's frame buffer, so a slow link is closed mid-backup

(Filed as "A USB host on the board closes its Bluetooth update link"; the
USB host turned out to be one way of meeting the mechanism, not its cause.)

**Symptom** — fixture C6 (`A0:F2:62:87:B4:8C`), image X
`a0a0a0a0+a3bf56bebe57`, Studio in Mac Chrome over Bluetooth, an update
that starts with a backup (the engine read back over channel 3):

- With `lp-cli link capture` hosting the board's USB link at the same time
  (run s1), the backup ran at 1.7 KiB/s for 17 s and then the board closed
  the Bluetooth link:

  ```
  radio link link4: a reply still not out of the frame buffer after 1584 ms (1588 B held; 1584 ms of the tick's wait budget left) — closing
  [ble] link4: closing at the server's request (reply deadline)
  ```

- With no USB host, two of three backups under a busy host (load 100–180)
  ended at 58–59 % with no board reset (runs a1d, a1e). The board's side of
  one such end (run d2, 2026-10-07, image `a0a0a0a0+81077deeeef7`, host
  load ~180) was caught by attaching the USB host only after the page saw
  the link go — the firmware's 4 KiB log ring keeps the newest records:

  ```
  [perf] frames p50≤50ms p99>2s max=3550ms(recv=0 tick=26 send=0 resp=0) >100ms=1 >1s=1
  … (one such frame in every 5 s window, 2.2–3.6 s, for the 40 s the ring held)
  radio link link2: a reply still not out of the frame buffer after 4633 ms (1402 B held; 4633 ms of the tick's wait budget left) — closing
  [ble] link2: closing at the server's request (reply deadline)
  ```

**Root cause** — a read-back answer (`D`, a whole 4 KiB chunk) is longer
than the radio link's `SMALL_REPLY_BYTES` (1024), so the board queues it as
the link's *external* message out of the one shared frame buffer, which the
link holds until it has cut the whole chunk into frames. Every other reply
on every link must wait for that before it can be serialized
(`release_radio_holders`): the server's heartbeat every 5 s on the same
Bluetooth link, and a USB host's answers. Each wait is the air time of a
4 KiB chunk — 2–3.6 s at the rates a busy Mac central gave, during which the
server loop renders nothing (the 2.2–3.6 s frames above: the heartbeat's
send sits outside the frame's recv/tick/send/resp breakdown). When the air
slowed further the wait passed the link's 5 s deadline (or, with a USB host
spending the tick's budget first, the 1.6 s left of it) and the board closed
the link. A buffer meant to hold one reply for one link's frame-cutting is
held across radio latency it does not control.

Also seen on d2, not established: the board's watchdog line
`[RECOVERY] io task silent > 2000 ms; withholding watchdog feed` once per
5 s window alongside those waits. With the fix applied (run e1 below) it
did not appear.

**Fix** — host side, no protocol or firmware change: over Bluetooth a
backup asks for `BLE_READ_BACK_PIECE` = 1016 B per `G`
(`lpa-update` `serve_session.rs`; `G` already carries its length and the
board already answers any length up to a chunk). The `D` is 1022 B, under
`SMALL_REPLY_BYTES`, so the board copies it into the link's own send ring
and never holds the frame buffer for it. The backup keeps four chunks'
worth outstanding (17 pieces, `ServeConfig::read_back_ahead`). USB keeps
whole chunks. A read-back piece unanswered for 20 s is also asked for again
(`BackupSession::reask_stale`, `READ_BACK_STALE_MS`): d2's resumed backup
then sat at 36 % for five minutes while the pieces after the hole kept
arriving, an answer lost with the link up that nothing re-asked.

Evidence, fixed build (`ed15e1091`/`e18e416fd`, `lp2025/2026-10-05-0820-ota-studio-ble-updates/data/ble-backup-2026-10-07/`):
run e1 attached `lp-cli link capture` to the board's USB for 40 s at 30 %
of a backup — the old s1 — and the Bluetooth link stayed up; the board's
frames meanwhile were `max ≤ 105 ms … >1s=0` in every 5 s window, and the
update finished with no click (426.5 s, load 32→113). In five full updates
with a backup on the fixed build (r1, r2, r3c, c1b, e1) no Bluetooth link
ended mid-backup; in the sixth (c3), cut at the central on purpose at 40 %,
the reconnected link ended once more 13 s later (no board log taken; cause
not seen) and the backup resumed both times.

**Regression coverage** — `lpa-update` `backup_session.rs`
(`pieces_smaller_than_a_chunk_ask_and_take_that_much`: every 1016 B piece's
`D` is ≤ 1024 B; `a_piece_whose_answer_never_came_is_asked_again_once_it_is_stale`),
`tests/backup_refused.rs` (`a_read_back_piece_lost_with_the_link_up_is_asked_again`,
and the Bluetooth backup's pieces in the resume test); `walk-ota-emu --ble
--steps cut-backup`. The board-side wait itself is the mux's own tests
(`a_radio_link_that_holds_the_frame_buffer_past_its_deadline_is_closed`):
unchanged — a long channel-1 reply can still hold the buffer.

**Lesson** — a size that decides which shared resource a message occupies
(here 1024 B: ring or frame buffer) is part of the protocol's behaviour even
when it is not part of its bytes. The host chose the read-back's size
without knowing it chose the board's buffer, and the cost showed up as a
deadline on a different reply, on a different link, under a different
load. When a resource is held across someone else's latency, look for who
sizes the hold.
