---
status: fixed
found: 2026-09-26      # how: the learned-wire-dictionary G1 sitting, agent-driven on a real XIAO ESP32-C6 (PR #835)
fixed: this change (PR #835)
area: fw-esp32-common serial::chunked_write (every chip's write loop) × lp-json-pack FrameScanner × lpc-wire WireStream
class: assumed-context
related:
  - docs/adr/2026-09-24-json-pack-wire-encoding.md
  - docs/adr/2026-09-25-learned-wire-dictionary.md
  - docs/defects/2026-09-24-the-real-c6-link-loses-bytes-inside-a-packed-frame.md
  - docs/defects/2026-09-26-an-emulated-board-went-silent-after-a-tab-reload.md
---
# A half-written packed frame swallowed the next connection's replies

**Symptom** — on a real ESP32-C6, Studio (headless Brave, real Web Serial)
connected to a board that a previous page had just been reading packed
replies from. The card read "Identifying", then "Unrecognized firmware" and
"Nothing from this board yet", while the board was talking normally: the
Web Serial capture (`?wire-capture=1`) held 29 KB of its output — the Hello
reply, heartbeats, log lines — and not one of them reached Studio.

**Mechanism** — the capture begins `\n 00 'L' <~60 bytes of a packed frame>`
and then plain text; it holds exactly one `00` in 29 KB. The previous page
closed while the board was part-way through writing a packed reply; the
write timed out (the host stopped draining) after some of the frame had
gone out, and those bytes were delivered to the **next** connection. A
packed frame ends only at its closing `00`, which never came: the board had
correctly fallen back to JSON for the new host, and JSON lines contain no
`00`. The new reader's frame scanner stayed inside the half frame and took
every later byte as frame body, until its 256 KB bound — minutes at
heartbeat rates. The framing assumed a frame, once started, is always
finished; a write abandoned mid-frame breaks that, and nothing on the host
side can tell a frame body from text that follows it.

It is older than learned frames: #795's static-dictionary frames have the
same framing. No earlier sitting reloaded a page mid-reply.

**Fix** — the board owes a **resync marker** after any write that failed or
timed out, and sends it before its next bytes: `00 00 'R' 01 00`
(`lpc_wire::RESYNC_SEQUENCE`; `fw_esp32_common::serial::chunked_write`,
the one write loop all three chips' replies and log lines go through). By
the scanner's own rules it ends in "reading text" from every state a torn
frame can leave it in (inside a body that is not valid COBS, inside one that
is, or not in a frame), and a marker itself cut short is simply sent again.
Readers drop the empty `'R'` frame silently.

**Regression tests** —
`lpc-wire wire_stream::the_resync_marker_frees_a_reader_left_inside_a_torn_frame`
(all three starting states, and the unfixed stream staying stuck);
`fw-esp32-common chunked_write::the_resync_marker_is_the_wires`.
