---
status: fixed
found: 2026-10-06      # how: e2e (PR C's emulated Wi-Fi walk, an upload over the LAN)
fixed: this change
area: fw-esp32-common `LinkMuxTransport` × lpa-server `AccessState` × fw-esp32c6 LAN endpoint (`lan_link_config`, `net_thread`, `LAN_LINK_SLOTS`)
class: budget-exhaustion
related:
  - docs/defects/2026-10-06-the-largest-block-probe-reads-a-64-kib-hole-as-65535.md
  - docs/defects/2026-09-24-ble-enabled-c6-refuses-a-project-switch-after-the-heap-cut.md
  - lp2025/2026-10-05-1903-wifi-link-c6 (PR B, #989; PR C's emulated walk)
---
# A LAN link strands the C6's heap below the load and read floors

**Symptom** — an emulated C6 joined to the virtual LAN
(`lp-emu:esp32c6:t1+net=lan`) refused `lp-cli upload projects/test/basic
lan:…`, first at the load gate
(`load refused: heap headroom too low (largest free block 65535 B < 65536 B)`),
and, once the probe was exact, at the read that follows a deploy:
`read refused: board memory busy (free 47388 B, largest block 14076 B; needs 40960 B free and a 16384 B block)`.
The same board joined but uploading over USB passed (74,120 B free,
36,375 B block after the load).

**Root cause** — three things a LAN link does to the heap, measured with
`heap_map_diag` and `heap_track_diag` (PR C's diagnosis):

1. **Stranding.** The first link to open grew four long-lived lists: the
   mux's `Vec<RadioLink>` (416 B) and `VecDeque<Incoming>` (480 B), and
   the access state's session map and key lookups. LLFF is first fit, so
   the growth landed above the link's own session and outlived it: after
   the link closed, the main region's free tail was split from 92,535 B
   into 16,006 B and 73,367 B. The mux's secure-event list was also handed
   away (`mem::take`) on every poll, so it regrew each time.
2. **Size.** One open link held its lp-link session (14,464 B at a window
   of 4), the endpoint's per-connection state (3,648 B) and, once the host
   asked for packed replies, a 6,904 B learned table.
3. **Placement.** The endpoint's TCP, WebSocket and mDNS buffers
   (about 17.5 KB with two slots) were allocated at the first address, after the boot heap
   had settled, wherever a hole was.

Two slots made it worse: with two links open during a load, the board
either refused the read (window 2: 47,692 B free, 14,724 B block) or, at
the window of 4, ran the shader compile out of memory and reset
(`[OOM] FRAGMENTED: 11224 B free in total but only 3088 B in one piece`).

**Fix** —

- Every per-link list is reserved at boot, at its most links
  (`LinkMuxTransport::new`, `LpServer::reserve_links`); the secure-event
  list is drained, not taken.
- A LAN link's window is 2 (TCP loses nothing; the session is about 6 KB
  smaller), and its replies stay JSON (the mux answers the packed opt-in
  `json`: no learned table).
- A board that boots with Wi-Fi on and a network saved allocates the
  endpoint's buffers in `net_thread::start`, on the boot path.
- One LAN slot on the C6, not the plan's two (`LAN_LINK_SLOTS`), saving
  the second slot's buffers and the second open session.

Emulated, `projects/test/basic`, before (PR C head `ae167ccf0`) and after:

| | before | after |
|---|---:|---:|
| idle, joined, after a link closed: largest block | 73,479 B | 97,688 B |
| used before the load, one link open | 156,416 B | 137,148 B |
| upload over the LAN | load refused (4 of 4 uploads) | passes (5 of 5) |
| project loaded, one link held: free / largest | — | 67,216 / 18,148 B |

**Regression coverage** — `link_mux_transport::tests::
a_lan_links_packed_opt_in_is_answered_json`; the slot count is followed by
`lp-cli/tests/lan_link.rs::a_link_past_the_boards_slots_is_told_to_try_again_later`
and the harness's `an_open_board_says_hello_on_a_secure_link_and_one_more_is_told_later`.
The memory figures themselves are emulator measurements, not gated: the
chip heap ratchet covers the boot heap, not a joined board with a link
open.

**Lesson** — on a first-fit heap a list that grows on an event allocates
at the event's moment, above whatever the event allocated, and keeps it.
The 2026-09-24 entry found the same mechanism for project-lifetime
containers; this is the link-lifetime version. Anything that grows the
first time a link or a project appears should be sized at boot. And the
read after a deploy runs with the link that carried the deploy still open,
so a link's whole footprint counts against that read's floor.
