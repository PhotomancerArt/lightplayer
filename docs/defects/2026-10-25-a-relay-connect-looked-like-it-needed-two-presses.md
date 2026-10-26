---
status: fixed
found: 2026-10-25      # how: Yona, setting up a new C6 joined to Wi‑Fi with Cloud relay on
area: Studio — the board card's connect through lightplayer.app (`connect-relay`, `connect`)
class: state-ended-at-the-wrong-event   # "connecting" ended at the socket's answer, not the board's hello
related:
  - lp-app/lpa-studio-core/src/app/devices/wifi_connects.rs
  - lp-app/lpa-studio-core/src/app/studio/studio_controller/account_boards_flow.rs
  - scripts/emu/walk-wifi-emu-studio-relay.mjs (T3, T5: one press)
  - docs/ux/anatomy/board-card.md ("While a road runs")
---
# A connect through lightplayer.app looked like it needed two presses

**Symptom** — signed in, a board on the account stood under Online boards
with "Connect through lightplayer.app". Pressed, it seemed to need a second
press before it connected, or to sit silent for a long while; its Project
row said "not known" until then.

**Cause** — the card's connecting state ended at the wrong event. A card's
Wi‑Fi or lightplayer.app connect (`WifiConnects`) was cleared when the
socket answered — through the relay, when the secure link came up — but the
board is only the card's once it says hello, which takes more round trips
through the relay. Between the two:

- a board this browser had met went back to its own Connect button, with a
  second "Identifying this board." card beside it for the relay link;
- a board known only from the account's list (`mac-<hex>`) never said
  "Connecting…" at all: its connect was not joined onto the account entry's
  card, and the entry was swapped for that identifying card the moment the
  relay link appeared.

So the press looked lost, and the button invited a second one. The
emulated walk on `main` (`just walk-wifi-emu studio-relay`, the walk's new
page-side timeline, local relay, `lp-emu:esp32c6:t1+net=lan`, lp-emu
1644367cb) measured it:

| | after the press |
|---|---|
| T3, a board met over USB | "connecting" at 14 ms; a second card at 49 ms; **its Connect back at 566 ms**; ready at 1,090 ms |
| T5, a board never met here | never "connecting"; its card gone at 47 ms, replaced by a separate identifying card |

On a local relay the gap is half a second; through lightplayer.app each step
of the hello is an internet round trip, so it is seconds.

"Project not known" was a second, smaller gap: a remembered board online at
lightplayer.app ignored the project name the account's list carries (relay
protocol 2 reports it in the clear), and said what this browser last heard,
often nothing.

**Fix** — a card's connect is under way from the press until a link of this
tab reaches the board (its hello), or a minute passes with no hello, when it
gives up in a held Connect's words ("The board didn't answer in time"). The
account entry's card carries its connect, and stands in place while it runs,
with the relay link's identifying card not drawn beside it. A second press
finds the button waiting and dials nothing. The board can say hello before
the relay's answer comes back to the page (the walk's T5 did): the hello
ends the connect either way, and an answer that lands later says nothing. A remembered board online at
lightplayer.app shows the project the board reports there.

**Regression tests** — `one_press_on_a_board_never_met_says_connecting_until_its_hello`
and `one_press_on_a_remembered_board_says_connecting_until_its_hello` (core,
pressed by offer path, every turn checked);
`a_hello_before_the_relays_answer_ends_the_connect`;
`a_remembered_board_moves_to_online_boards_while_the_list_names_it` (the
project); the studio-relay walk's T3 and T5 now fail on a second press, a
"connecting" later than a second, a pressable Connect or a second card while
it connects, or an offline/online face after it.
