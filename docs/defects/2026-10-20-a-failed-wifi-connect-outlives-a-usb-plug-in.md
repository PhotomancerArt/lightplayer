---
status: fixed
found: 2026-10-20      # how: Yona, a known board that was off, Connect over Wi‑Fi, then its cable
fixed: 2026-10-20
area: lpa-studio-core `WifiConnects` (a known board's failed connect) and `StudioController::fold_device_input`'s settle step
class: lifecycle-ownership
related:
  - docs/defects/2026-10-19-a-re-enumerating-board-remounts-its-card.md
---
# A failed Wi‑Fi connect kept covering a card after the board was plugged in

**Symptom** — Yona: a board Studio knew, switched off; Connect over Wi‑Fi
failed; then the board was plugged in over USB. It connected and worked,
and the card still said "Studio couldn't reach this board over Wi‑Fi." over
its picture.

**Root cause** — `WifiConnects` kept a failed connect "until the next
press", and a board reached by another road is never offered Connect over
Wi‑Fi, so there was no next press. The card's surface puts a network failure
above every state but the ones in progress, so it covered a live board. The
USB Connect's failure had its own settle step (`settle_connect_failure`:
the words go once the board is reached by another road); the Wi‑Fi and
lightplayer.app connects had none.

**Fix** — `WifiConnects::settle_reached` drops a known board's failed
connect (on either road) once a link of this tab reaches the board, run on
every device fold beside `settle_connect_failure`. A connect still under way
is left to finish, and the address field's answer is no board's. Unplugged
again, the old failure does not come back: the card offers Wi‑Fi afresh.

**Test** — `wifi_connect_tests::a_failed_wifi_connect_goes_quiet_once_the_board_is_plugged_in`
(the core device bench: a fake LAN board refuses, the USB grant returns),
and `wifi_connects::tests::a_failure_goes_once_its_board_is_reached_and_a_connect_under_way_stays`.
