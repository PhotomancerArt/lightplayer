---
status: open
found: 2026-10-09      # how: e2e (`just walk-two-tabs-emu`, its first runs)
area: lpa-studio-core `device_effects.rs` (`request_grant`) × `studio_controller/board_hold_flow.rs` (`read_refused_ports`) × `take_over_flow.rs`
class: partial-knowledge-loss
related:
  - docs/adr/2026-10-08-the-board-card-and-one-home-page.md (the amendment of 2026-10-09: the chooser path)
  - lp2025/2026-10-08-2330-one-tab-holds-a-board (p3-usb-holder-side.md, p4-usb-asker-side.md, p7-emulator-and-walk.md)
---
# A chooser pick of a held USB port is not gated, and a quick Connect on it never recovers

**Symptom** — In tab B, with tab A holding the board, "Connect a board",
USB, pick the board:

- B asks the door for the port twice and is refused both times
  (`NetworkError`, `bus.openAttemptsFor` 2 after the pick; the walk's step 2
  reports it). A port granted at load never does: the sweep's gate attaches
  it without opening it.
- Until the refused identify settles (the 5 s deadline), B's page holds two
  cards for one board: the board's own, "Open in another tab" with Connect,
  and a "New device found — identifying…" card for the port. When the
  identify settles, the refusal is read against A's claim and the port folds
  onto the board's card.
- If the user presses Connect (the take-over) **before** that reading, A
  lets go and B ends with a "No response" card: the board under Offline
  boards, Install as its primary, "No response" in the firmware bar, and B
  holding nothing. The walk's first run did exactly that, because it pressed
  Connect a moment after the pick. The board is free and B never opens it.

**Root cause** — `request_grant` registers the port the chooser returned
with no `gate_decisions`: it cannot gate it by count, because the user may
have picked a different board of the same kind (one claim and one picked
port is either board). So B opens it, the OS refuses, and the refusal is
read against the claims only when the identify settles Failed
(`read_refused_ports`: `settled_failed` and the claims at that moment). A
take-over that releases before then clears the claims, `release_pair` returns
only the links the gate holds (`gated` and `read_held`), and the refused
link, which is neither, is neither re-identified nor ever read as held. The
claims were known when the port was picked, and again when the open was
refused; the reading waits for a deadline and loses them.

**Fix** — none yet (reported, not fixed here: M5's last phase is docs and
walks). Candidates, smallest first:

- read the refusal when the open is refused (`LinkEvent::Error` with the
  OS's `NetworkError`), not at the identify deadline, so the claims are the
  ones in force;
- have a take-over's release also re-identify pending links of the pair
  that settled Failed with their port not open;
- in the shim, share the grant with the first page under
  `?emu-second-tab=1` (real Chrome lists a granted port in both tabs), so the
  walk's second tab meets the sweep and not the chooser. That models Chrome
  and leaves the product path as it is.

**Regression coverage** — none for the unread order. The walk waits for the
reading (one card for the board) before pressing Connect, and asserts that
none of B's opens succeeded while A held the board
(`walk-two-tabs-emu`, steps 2 and 3). Core's T1 and T3 cover the sweep's
gate (zero opens).

**Lesson** — A refusal is evidence that expires: read against the claims
when it arrives, not when a timer says it has settled, or the claims that
explain it may have changed by then.
