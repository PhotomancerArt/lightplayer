---
status: fixed
found: 2026-10-21      # how: the emulated walk (`just walk-wifi-emu studio-relay-move`, T6), before merge
area: lpa-devices device.rs (a device folds every link it was heard on) × lan_move_flow.rs
class: state-conflation
related:
  - docs/adr/2026-10-08-studio-network-links.md
  - lp2025/2026-10-20-0845-best-link
---
# A board moved onto Wi‑Fi went offline when its relay link left

**Symptom** — The automatic move took a board's session from
lightplayer.app onto its LAN link: the board said its LAN link took the
network slot over, the page said "Moved to Wi‑Fi on your network", and the
editor was rebound onto the LAN link. A third of a second later the editor
said "the connection dropped; the editor stays open while it reconnects",
and 5 s later the page went back through lightplayer.app, whose route the
board then turned away as busy (the LAN link still held the slot). The core
tests had passed: they never asserted the editor after the relay link left.

**Root cause** — A device is routed every link it was heard on (the
roster's route map), and its evidence keeps one presence. The page let the
relay session go once the board was on Wi‑Fi; the departure sweep then
detached the relay link, and the device folded that `LinkDetached` as its
own: presence detached, though its own link (the LAN's) was open. The
editor's "is the wire still there" check read the presence and held.

**Fix** — `Device::handle_event` drops the close or the departure of a link
the device is not on, while it is on another, and journals it
(`JournalNote::StaleLinkEvent`). The link it is on still takes it offline
when it goes. Pinned by `roster::tests::the_link_a_board_left_closing_or_going_away_leaves_it_on_the_new_one`
and by `lan_move_tests::an_idle_relay_session_moves_onto_wifi_with_the_editor_open_and_no_curtain`,
which now ticks past core's 5 s after the relay link leaves and asserts the
editor was never held and nothing went back; it fails with the fix removed.

The same walk found a second way to lose the link: clearing the relay's key
order right after the move changed the LAN link's best key, the link
rekeyed, and the board closes a keyed session that restarts ("a new session
is a new link"). The order now lasts as long as the board is on Wi‑Fi.

**Lesson** — "Last hello wins" said which link a board is on, but the events
of the other link still landed on the same presence. When one record is fed
by two sources, each event has to say which source it is about, and the
record has to ignore the one it is not on. The bench's doubles modelled the
sockets and not the departure sweep's timing, so the test that would have
caught it only asserted the move, not the minute after it.
