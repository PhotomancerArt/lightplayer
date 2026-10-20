---
status: fixed
found: 2026-10-10      # how: the connected feel gate's preview restart (?emu=tab); finding 8
fixed: this change
area: lpa-studio-core `StudioController::connect_failure` × the connection bar's session work
class: lifecycle-ownership
related:
  - lp2025/2026-10-08-2330-connected-in-the-card (the connected feel gate, finding 8)
---
# "Couldn't connect" stayed after the board was reached

**Symptom** — a Connect gave up on an offline board ("Couldn't connect ·
Retry" on its connection bar). The board was then reached another way: the
USB square, a fresh install and a project. It was Ready and running, and
the bar still said "Couldn't connect".

**Root cause** — the failure was cleared only by the next Connect, Edit or
Done. Nothing owned clearing it when the board itself moved on, so it
outlived every fact it described.

**Fix** — a failure remembers the link the board was on when it failed
(`ConnectFailure::link`) and goes once the board is ready on another link,
reached by another road (`ConnectFailure::outlived`, run after every device
fold). A failure on the link the board is still ready on stands: Retry still
means something there.

**Regression coverage** —
`connected_board::tests::a_connect_failure_is_outlived_by_the_board_ready_on_another_link`,
`connected_tests::a_failed_connect_clears_once_the_board_is_reached_another_way`.

**Lesson** — every sticky "it failed" needs an owner that clears it when the
world it describes is gone, not only the next press of the same verb.
