---
status: fixed
found: 2026-10-09      # how: e2e (`just walk-two-tabs-emu`, its first runs)
fixed: this change     # PR #1121
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

**Fix** — the first two candidates this entry listed, in the product;
the chooser stays ungated, and the shim does not share the grant under
`?emu-second-tab=1` (that would have hidden the product path from the
walk):

- **The refusal is read when it is heard.** `fold_device_input` notes a
  `LinkEvent::Error` on a pending USB port whose identify asked for the
  open and that did not open (`note_refused_open`), and the batch's
  reconcile reads it against the claims standing then
  (`read_refused_ports`, `BoardHoldFlow.refused`): one claim and one port
  of the kind name the board, `Event::LinkHeld` settles the identify at
  once, and the port sits on the board's own card. The deadline reading
  stays for a port that never answered at all.
- **A take-over's release opens the refused ports of its kind.** When a
  USB take-over's holder lets go, every pending port of the kind that this
  tab could not open — kept shut by the gate, read as held, or settled
  Failed with its port not open — re-identifies, once per take-over
  (`open_ports_take_overs_free`, `FreedPorts`). A refusal heard after the
  release (the open went out before the holder closed; the walk's order
  makes that likely, since the browser retries a refused open after
  250 ms) is that hold's: it is read as held and opens again too.
- A holder that lets go by itself (no take-over) still opens nothing here
  (R3): the fact clears, the port loses its mark, and the card offers
  Connect.

**Regression coverage** — core's two-tab rows
(`studio_device_e2e_tests/board_hold_tests.rs`), each failing before the
fix:

- `g1_a_picked_port_another_tab_holds_reads_as_held_at_once`: the pick is
  read within half a second of fake time (it took 5.02 s, the identify
  deadline), one card, "Open in another tab"; when the holder disconnects
  the card offers Connect, nothing opens until it is pressed, and it opens.
- `g2_connect_right_after_the_pick_takes_the_board_over`: Connect pressed
  the moment the open is refused ends Ready on the board's card (it hung).
- `g3_a_refusal_heard_after_the_release_opens_again`: the release heard
  before the refusal ends Ready too (it hung).

`just walk-two-tabs-emu --serve-release` no longer waits for the reading
before step 3 presses Connect; it checks one card for the board after the
take-over instead.

**Lesson** — A refusal is evidence that expires: read against the claims
when it arrives, not when a timer says it has settled, or the claims that
explain it may have changed by then.
