---
status: fixed
found: 2026-09-25      # ci — reddened main at a7daa7ef5 (#805's merge), run 36091855810
fixed: this change
area: scripts/emu/lab (server.mjs scheduler × test/queue.test.mjs, notify.test.mjs, stability.test.mjs, server.test.mjs)
class: unenforced-test-precondition
related: [2026-09-24-boot-no-radio-asserted-the-window-edge.md]
---
# The emu-lab queue tests measured the cooldown on the wall clock

**Symptom** — `Emulator C6 (x64)` failed at `just test-emu-lab` on the main
push of `a7daa7ef5` (the #805 merge, run 36091855810):

```
not ok 33 - cooldown: a press shorter than the floor waits the floor (C)
  error: 'a 5 ms press still rests the 600 ms floor (waited 504 ms)'
```

#805 touched nothing under `scripts/emu/lab`, and the next main runs passed.

**Root cause** — two unestablished conditions in the test, not a scheduler
bug.

1. *Measured at the wrong place.* The server enforces the cooldown between
   two of its own instants: the press's `resultAt` (when it handled the
   result POST) and the next press's `sentAt`. The test measured between two
   instants on the fake device: `answeredAt`, when that device's result POST
   *resolved*, and `sentAt`, when the next press arrived over SSE. Under load,
   the POST resolves late. The server has already stamped the end, answered,
   and started the cooldown clock, but the test process has not been
   scheduled to see the response yet. Every millisecond of that lag comes off
   the measured gap. So a server that waited the full 600 ms read as 504 ms.
2. *Measured on a clock nothing controlled.* Every timing assertion in the
   suite had a wall-clock bound in one direction or the other: gap ≥ 520,
   ≤ 1600, ≤ 700, ≤ 400, `gap >= 150 - 25`. The notifier tests slept 300 ms
   and then counted lines a shell had to write. The drop-flicker test needed
   a reconnect inside 1.5 s of real time. The shared lab's 400 ms lost bound
   re-sent any press that a loaded runner took too long to answer. Each
   tolerance was a guess about how busy the machine would be.

Reproduced locally at load average ~100 on a 12-core M2 Max, with 8 copies of
the suite in parallel for 3 rounds on `origin/main` (`0b73e5285`). Failures:
5 of 24 runs. Three were the notify grace test, and two were the first
queue test (`press 2 sent 102 ms after the previous END (spacing 150)`).
Every run that failed that first test also failed the `waits` test after it.
The likely cause is that the failing test threw before it stopped its fake
device, which then stayed joined to the shared lab. The CI failure's own
test did not trip in that sample, and it did not need to. Its mechanism is
the same as the spacing test's: a client-side gap taken on a loaded machine.

**Fix** — the scheduler now reads an injected clock. `scripts/emu/lab/clock.mjs`
exports `realClock`, which is `Date.now()` and node's timers unchanged, and
`createManualClock`. `server.mjs` takes every instant the queue reasons
about from `clock` rather than `Date.now()`: press sent and end, spacing,
cooldown, the lost and drop bounds, TTL, the notifier's grace and floor, and
every timestamp it writes. The tick interval runs on that clock too.
Transport timers stay real: the SSE keepalive, a `/wait` timeout, and the
exit after SIGTERM. In life nothing changes. Under `LAB_CLOCK=manual` (tests
only), time stands still until `POST /test/clock {advanceMs}` moves it. That
call fires the timers that fell due, then runs one tick. The route is a 404
otherwise, and a test pins that. The fake device spends its `pressMs` by
advancing the clock, so the server measures exactly the burn it was given.
The timing tests now assert equalities read off the server's own press
record. The floor test checks that press 2 is still `pending` at 599 ms,
sent at 600 ms, and that `resultAt → sentAt` is exactly 600. The spacing,
ceiling, proportional, off, restart, lost, drop, flicker and notify-grace
tests follow the same pattern. The notifier gained a `sending` count in
`/status`. Its sends are fire-and-forget from the tick, so a test counts
lines only once nothing it caused is still in flight. The shared real-clock
lab's lost bound is now 60 s, so a slow answer cannot re-send a press.

**Regression coverage** — `scripts/emu/lab/test/clock.test.mjs` tests the
clock itself (due order, intervals across periods, timers set from timers,
refusals). `/test/clock does not exist on a server on the real clock` in
`server.test.mjs` pins the production route surface. The converted tests are
now sensitive enough to catch a 1 ms change. Lowering the floor clamp to
`FLOOR - 1` fails `a press shorter than the floor waits the floor` with
`press 2 still waits 1 ms before it is owed`. The old test tolerated an 80 ms
error in either direction. Under load on the same desk, same day:

| suite | parallel copies × rounds | load avg | failed runs |
| --- | --- | --- | --- |
| before (`0b73e5285`) | 8 × 3 | ~100 | 5 / 24 |
| after | 8 × 3 | ~100 | 0 / 24 |
| after | 16 × 3 | ~159 | 0 / 48 |
| before (`0b73e5285`) | 16 × 3 | ~186 | 11 / 48 |

The last row ran right after the third, so its load was somewhat higher. It
failed the same three tests as before. Five back-to-back serial
`just test-emu-lab` runs after the fix passed 80/80 each.

**Lesson** — a test of a rule about time must run on the clock the rule is
enforced on, and read its answer where the rule is enforced. Measuring from
the far side of a socket adds the scheduler's lag to the thing measured, and
no tolerance can say how big that lag gets on somebody else's machine. The
class's own advice ("throttle by quantity of work, never by elapsed time")
has a direct form for code whose job is elapsed time: inject the clock and
step it. A wall-clock timeout is still fine as a liveness bound (`until`),
provided no assertion depends on how long the wait took.
