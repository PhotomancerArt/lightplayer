---
status: open           # mitigated for Studio tabs of one browser (M5's hold channel)
found: 2026-10-09      # how: report (planning survey, "One tab holds a board" F3; not seen on a walk)
area: browser_websocket.js (rule 2) × fw-esp32-common parked_handshake × fw-esp32c6 lan_endpoint_task
class: state-conflation
related:
  - docs/adr/2026-10-07-c6-wifi-link.md
  - docs/adr/2026-10-06-cloud-relay.md
  - lp2025/2026-10-08-2330-one-tab-holds-a-board (notes.md F3, p5-network-slot.md)
---
# Two clients with one key take a board's network slot from each other

**Symptom** — Latent: no walk has shown it, and no test reproduced it
before M5's. Two clients that present the same key to one C6 over the
network trade its one network slot back and forth with no gesture: two
Studio tabs of one browser on a keyed board, or a Studio tab and lp-cli
holding the same key. Each one's connect closes the other, and the closed
one redials within a quarter of a second and closes the first. Neither
keeps a link long enough to do anything.

**Root cause** — One close means two things. The board gives its network
slot to a newcomer whose first handshake message verifies under the
holder's key. `parked_handshake.rs` says "the holder is closed", and
`lan_endpoint_task.rs:271` closes it with an ordinary
`ws.close(CloseCode::NORMAL)`. The page's provider reads any close it did
not ask for as a departure: rule 2 of `browser_websocket.js` says "a drop
is a departure, then a reconnect with no gesture" (250 ms, then 1, 2, 4, 8,
15 s, the last repeating). The redial presents the same key, so it takes
the slot back and closes the newcomer, whose provider redials in turn.
"The board went away, come back" and "another client with your key took
it, stay away" arrive as the same close.

Every tab of one browser presents the same keys (the account's and the
browser's, `network_link_keys.rs`), so two tabs on a keyed board are the
common case. On an open board every Studio is anonymous, and a second
client is refused with 1013 instead, so it never loops.

**Fix** — Mitigated for Studio tabs of one browser only, by M5's hold
channel (`lpa-studio-core`, `studio_controller/board_hold_flow.rs`, "the
yield"):

- A tab that holds a board's network slot announces it
  (`lp-board:net:<mac>`).
- A tab that hears another tab newly announce the slot it had closes its
  own session by request. Its open link goes with `Action::Disconnect`. A
  session the board already dropped, which would redial, goes through the
  transport's forget.
- No redial follows. The tab's card says the board was taken.

Nothing announces for a client in another browser or for lp-cli, so the
ping-pong stays there. Ending it needs a close reason from the firmware
that says "taken over by your own key", and a provider that does not
redial on it. Both are firmware and provider changes that M5 may not make
(future work in the plan's `notes.md`).

**Regression coverage** — for the tabs of one browser:
`board_hold_tests/network_hold_tests.rs`:

- `n3_a_tab_that_hears_another_take_its_network_board_yields_and_never_redials`:
  the drop arrives first.
- `n3_when_the_holds_note_beats_the_drop_the_holder_closes_its_open_link`:
  the other tab's note arrives first.

Both run over a scripted network slot whose sessions redial as rule 2
says. Each asserts exactly one close by request and no redial. Both fail
with the yield removed. Nothing covers another browser or lp-cli. A two-tab
Wi‑Fi walk was not run: P7's walk is USB, and an open board refuses an
anonymous second tab by design.

**Lesson** — A close is a message, and the code at each end of it decides
what it means. When one side has two reasons to close and sends the same
code for both, the other side picks one meaning for both. Here it picked
"come back", and two peers that each come back make a loop. Any
"newest wins" slot needs the loser told that it lost, in a form its
reconnect logic reads.
