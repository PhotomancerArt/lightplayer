---
status: fixed
found: 2026-10-06      # how: e2e (PR C's emulated Wi-Fi walk, Studio's editor on a LAN link)
fixed: this change
area: fw-esp32-common `LinkMuxTransport::release_radio_holders` × fw-esp32c6 RWDT (`recovery::watchdog`, 8 s)
class: timeout-scoped-to-sub-phase
related:
  - docs/adr/2026-09-24-ble-transport.md (decision 3, the shared frame buffer)
  - lp2025/2026-10-05-1903-wifi-link-c6 (PR B, #989)
---
# A stalled radio link holds the server tick past the watchdog

**Symptom** — an emulated C6 with Studio's editor on a LAN link logged
`Project loaded: studio`, then
`radio link link1: a reply still not out of the frame buffer after 5000 ms (2048 B held) — closing`,
`[lan] link link1: closed (reply deadline)` and `[perf] … tick=5040ms`.
It repeated on every redial until `rst:0x10 (LP_WDT_SYS)`, 12 times.

**Root cause** — every reply is serialized into one shared frame buffer.
Before writing it, the server tick awaits every radio link that still
reads the buffer, for up to `RADIO_WRITE_DEADLINE_MS` (5 s) **each**. The
deadline was per wait, not per tick. A tick that had already loaded a
project and compiled its shader, then waited out a stalled peer, ran past
the 8 s watchdog, which the loop feeds once per tick. The emulator's fast
guest clock made the LAN peer look stalled, but the hazard is the
firmware's own: a host that stops reading (a backgrounded tab, a stalled
Wi-Fi, a Bluetooth central out of range) reaches it on silicon too. With
three radio slots, three stalled links could wait 15 s in one tick.

**Fix** — each wait is capped twice: by its link's deadline (Bluetooth
5 s, its slow air's measured need; LAN 1 s, since a LAN peer drains 16 KiB
in milliseconds), and by what is left of the tick's own
`TICK_WAIT_LIMIT_MS` (5 s), counted from the last upkeep and including
whatever the tick did before. A link past either is closed at once, and
the tick goes on. Bluetooth shares the mux, so it gets the fix too. The
render still pauses for up to the deadline while a peer stalls; it no
longer pauses without a bound.

**Regression coverage** — `link_mux_transport::tests::a_tick_past_its_wait_budget_closes_a_stalled_link_without_waiting`
(no wait at all once the budget is spent; the link is closed) and
`a_stalled_links_wait_is_its_own_deadline` (5 s on Bluetooth; the LAN
slot's deadline is 1 s). Not covered: an end-to-end watchdog run, since
the host tests have no RWDT.

**Lesson** — a deadline named for "the device cannot stall" was a
deadline on one wait. What the watchdog bounds is the tick, so the budget
has to be the tick's. Every await on the server loop's path is a share of
one 8 s allowance, not a fresh one.
