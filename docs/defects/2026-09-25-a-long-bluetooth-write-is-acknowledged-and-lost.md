---
status: fixed          # confirmed on the phone (Yona, 2026-09-25) with the firmware half alone
found: 2026-09-25      # how: hardware-walk (G4, Yona, iPhone + Bluefy on deployed Studio) then live-debugging (choker console; Mac Chrome over CDP)
fixed: this change     # PR #834 (claude/ble-phone-edit-drop): fb0b2ed59 (firmware), 6676bf59e (Studio transport)
area: fw-esp32c6 ble (ble_connection, nus_service, trouble-host packet pool) × fw-esp32-common radio_link::prepared_write × lpa-link browser_ble.js
class: untested-path
related:
  - docs/defects/2026-09-25-a-knob-jump-over-bluetooth-kills-the-c6-ble-host.md
  - docs/adr/2026-09-24-ble-transport.md
  - lp2025/2026-09-23-1428-ble-remote-control (director-log A23)
---
# A long Bluetooth write was acknowledged and lost, and a page-side drop never reached the radio

**Symptom** — at G4, on an iPhone in Bluefy (deployed Studio, main
`ccd19721a`+), the PLAYFUL choker connected over Bluetooth and knobs worked,
but **editing any setting sent Studio back to `/devices`, with no error on
screen**, every time. A reconnect then said "speaks the framing but never said
hello (pre-hello firmware)", and the board seemed to need a reset. The Mac
(USB, and Mac Chrome over Bluetooth) was fine.

A non-resetting console reader on the board (10:BD:A3:B0:A5:2C) during two
phone repros showed what the board saw. The first repro ran on a proto-26
image, `acabd97c1`, not the `ccd19721a` G4 assumed; the second ran on
matching `a34cfee65`:

- **Nothing went wrong on the board.** There was no host error, no parse
  warning and no disconnect. The only warning in the proto-26 session was
  Studio's `accessList` request, which that image cannot parse. It dropped
  the request without answering: `dropping unparseable 36 B M! line (unknown
  variant accessList …)`.
- **The radio link outlived Studio's "drop".** Each session's link (`link12`,
  then `link1`) stayed open after the bounce. It closed only when the tab was
  closed, ~20 minutes later the first time:
  `[ble] link12: disconnected, reason=0x13`. So Bluefy told the page the link
  was gone while iOS kept the radio connection. The page went back to
  `/devices` through the editor's dead-wire backstop or the lens-closed
  route: requests got no answer, but the board's unsolicited heartbeats kept
  arriving. A reconnect then rode the old link. The board saw no new link,
  so it never sent the hello it owes one, and the page said "never said
  hello".

The phone's own console was never captured (Bluefy has no dev tools; neither
Safari Web Inspector nor a Tailscale-served lab was run). Chasing it on the
Mac found the two firmware defects below, both of which make a page request
vanish with the board silent. **The phone confirmed the cause.** With only
the firmware half flashed and the deployed Studio (still 244 B writes, no
teardown), Yona connected from Bluefy and "now everything seems to be
working": editing held. The firmware half has two changes: long writes are
reassembled, and the ATT MTU is 247 rather than 251. The phone run does not
say which one it needed. The likely reading is that Bluefy's larger writes
were going out as long writes and vanishing, even though the board had
logged the phone's ATT MTU as 251, which a 244 B write fits. Why iOS would
choose a long write there is not known.

Seen once, not explained: on that confirmation the phone's first connect said
"no response" and the retry worked. At the time the agent's lab page (Mac
Chrome) held the board's other slot and was reconnecting every ~12 s (each of
its unauthenticated links was closed at 10 s), so it competed for the
advertising slot. No board log of that moment survived (the lab's console
reader had stopped).

**Root cause** — three mechanisms.

1. **trouble-host 0.6 answers a long write itself and keeps the bytes.** A
   central may write a value longer than MTU − 3 as a run of `Prepare Write
   Request`s and one `Execute Write Request`. trouble-host answers both
   through its attribute server. It writes each segment into RX at offset 0
   and replies "success" when the segment fits RX's 244 B, or an error when
   it does not. Neither kind reaches the connection as a
   `GattEvent::Write`, the only event `ble_connection` took RX bytes from.
   So a long write whose segments fit was **acknowledged and thrown away**,
   and one whose segments did not was refused with no line in the board's
   log. Either way the request never reached the server, and nothing on the
   board said so.
2. **The ATT MTU was bigger than the controller's packets.** The host's ATT
   MTU is its packet size − 4, and the pool was 255 B, so the MTU was 251.
   The C6 controller's largest ACL packet is 251 B. A reply of a full MTU
   (251 B ATT + 4 B L2CAP = 255) failed trouble-host's send, and that failure
   **restarts the whole BLE host**. Found by the fix's first desk run: a
   246-byte Prepare Write, echoed back in full, printed
   `[host] error sending outbound pdu` then `[ble] host restarted —
   advertising again`. Notifications (244 B values) always fit, which is why
   nothing hit this before.
3. **Studio's drop never touched the radio.** `browser_ble.js` handled a drop
   (`gattserverdisconnected`, a failed write with `gatt.connected` false, or
   a visibility re-check) by marking the session lost and reconnecting. It
   never called `gatt.disconnect()`. It also ignored a failed write while the
   browser still called the link up: the rest of that line was lost, and the
   half line left in the board's joiner poisoned the next one.

**Fix** —

- Firmware: `radio_link::prepared_write::PreparedWrite` (host-tested) queues
  a connection's prepared segments, which must be in order and at most 512 B
  in all. The whole value goes to the line joiner on `Execute Write`
  (flags 1), and flags 0 cancels. An out-of-order segment or an oversized
  value is refused with `INVALID_OFFSET` / `PREPARE_QUEUE_FULL`, and the
  refusal is logged (`[ble] linkN: long write refused …`).
- Firmware: the trouble-host packet pool is 251 B
  (`default-packet-pool-mtu-251`), so the ATT MTU is 247 and no ATT PDU can
  outgrow the controller. Notifications stay 244 B values, so nothing else
  changes size.
- Studio: rule 5 in `browser_ble.js`. Every drop calls `gatt.disconnect()`,
  and so does any failed write, so a reconnect is always a fresh link and the
  board owes it a hello. Writes are chunked at **180 B** (was 244), inside one
  ATT value at iOS's common MTU of 185 as well as the board's 247, so no
  write depends on the long-write path. Measured on the Mac: 2 KB page to
  board, with response, at 15 ms, 244 B writes 2.5–6.4 KB/s (median ~3.9),
  180 B 2.9–5.3 KB/s (median ~3.2).

**Regression coverage** —

- `fw-esp32-common` `radio_link::prepared_write::tests` (6): in-order
  segments come out whole, MTU-sized segments, cancel, gaps/repeats refused,
  the 512 B bound, empty execute.
- `lpa-link` `browser_ble_conformance`:
  `a_phantom_drop_is_torn_down_and_the_reconnect_is_a_fresh_link`. The
  `?ble=emu` polyfill's new `phantomDrop` models Bluefy, and the test fails
  with the teardown removed. `a_long_line_goes_out_in_awaited_180_byte_writes`
  pins the chunk size.
- Desk: `spikes/ble-lab/scripts/m4-desk-check.py --only-long-writes` (also
  step 7b of the full run). It sends `hello` padded to 300/400/512 B as ONE
  write, and 600 B in 180 B and 244 B writes. All must be answered, with no
  drop and no host-fault line. On the choker with the fix: all five answered
  in 360–465 ms. Before the fix, the long writes failed ("GATT operation
  failed for unknown reason"), and in the first fix build (RX grown to 248 B
  on the 255 pool) a 300 B one restarted the host.
- Silicon, Mac (choker, Mac Chrome over CDP, clean image `f7b722bfa`, board
  open so Play tier): 10 Scale panel writes in a row, each ONE 300–480 B
  write (a long write), all `accepted`, 240–831 ms. Then the page dropped
  the link (`link1 … reason=0x13`), reconnected as `link2` in 1,077 ms, and
  its hello was answered in 122 ms. No reset.
- Phone: confirmed working by Yona with the firmware half (above). The
  Studio half (teardown, 180 B writes) reaches the phone at the next deploy.

**Lesson** — a BLE host library that "handles" a GATT procedure for you can
still leave you out of it. trouble-host's attribute server answered Prepare
and Execute correctly on the wire and told our code nothing, so a silent
success was worse than an error. When a transport's peer can do something our
code does not handle (long writes, a peer-side disconnect the page never
reports), probe it from the desk with a central that does it. And a
transport's "the link is gone" must make it gone at the radio, because a peer
that disagrees will happily keep the old one.

Separately, the board dropped an unparseable request (`accessList` on a
proto-26 image) with no answer, so Studio waited on it. Studio also talked to
a proto-26 board from a proto-27 page without clearly saying "this board needs
a firmware update". Both are wire-skew UX, recorded here for the wire-skew
plan to own. They are not fixed in this change.
