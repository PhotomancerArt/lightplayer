---
status: fixed
found: 2026-09-13      # live-debugging (W7 of lp2025/2026-09-11-0911-tab-emulator-loose-ends)
fixed: 56d543cdd    # the commit before the entry's own, in this PR
area: lpa-studio-web/public/lpa-link/emulator_worker.js (the tab backing's pacing loop)
class: bound-in-a-foreign-unit
related:
  - 2026-09-11-a-reset-replays-the-boards-lifetime.md
  - ../adr/2026-09-09-studio-device-stack-over-a-virtual-serial-port.md
  - lp2025/2026-09-11-0911-tab-emulator-loose-ends/w7-first-flash-stall-and-walk-on-main.md
  - lp2025/2026-09-10-1707-c6-emulator-in-tab/G2-handoff.md
---
# The dilation window is bounded in wall time and fed per loop iteration, so retiring it froze the board for 21 s

**Symptom** — a Flash pressed on a `?emu=tab` board that had been up and
running firmware waited out `emulator_tab.js`'s 30 s request deadline. A15
measured it on mode A (G2-handoff, question 4, amended post-W6): *Update
firmware* → `Flashing firmware…` → `Ready` in **33.7 s**, of which 30 s was
the hub refusing a control line by name —

```
[emu-link] board dev000000ae1zxd4909: `rts 0` → err the emulator worker did not
answer `control` (request 2) within 30 s
```

— while the same board's next Flash, from the blank face, took 4.0 s.

Reproduced on mode B (`?emu=tab`, esptool-js over the virtual port) on
`origin/main` `77e172c3f`, five bounded runs in headless Chrome over CDP on
this worktree's own hashed port (36046 — `dev-port.sh`'s number, not one
carried from a document). The script's shape is A15's condition: press
*Flash firmware* on the blank `tab-c6`, wait for `Ready`, age the board 65 s
of wall, then press *Update firmware*. **The verb did not settle inside 120 s
in three of four runs, and took 49.8 s in the fourth** (the fifth run timed
the worker's own phases). The walk's `upload` step failed the same way on the
same code: `waiting for the board to say 'Project loaded': wait deadline`, in
two of three `just walk-no-board --tab` runs on unmodified main.

**Root cause** — `sample()` in `emulator_worker.js`:

```js
function sample(wallAt) {
  dilationWindow.push([wallAt, Number(emu.micros())]);
  while (dilationWindow.length > 2 && wallAt - dilationWindow[0][0] > DILATION_WINDOW_MS) {
    dilationWindow.shift();
  }
}
```

The window is **bounded in wall milliseconds and fed one entry per pacing-loop
iteration**, and the two units decouple completely as soon as the guest runs
*ahead* of the wall. A flashed, idle LightPlayer board does exactly that: the
machine fast-forwards its `wfi`, so one 6.4 M-cycle slice returns in **7–8 ms
of wall having advanced 40 ms of guest time**. The pacing rule's first branch
then hands out no cycles at all — `if deficit <= 0: await tick(); continue` —
and the loop spins on a `MessageChannel` tick that costs **0–1 ms**, pushing a
sample every time round. Measured: **193 858 entries inside one 1 000 ms
window**.

When the second finally does elapse, the trim retires that prefix with one
`Array.prototype.shift()` per element — O(n) each, O(n²) for the drain. That
is the freeze, and it is the *whole* of it:

```
step 16484 ms = run 7 + drain 0 + report 16476
                [sample 16476 (win 122246) + counters 0 + dilation 0
                 + hasImage 0 + translation 0 + post 0]   (tick 0)
```

And because a Worker can only apply an inbound message between slices — the
machine's own rule, and this file's header says so — the thread's inbox is
shut for that whole time. `hub.request` timings from the same run show what
that costs the party waiting: `control` requests taking **21 246 ms**,
**20 226 ms**, **15 758 ms**. Past 30 s the hub rejects by name, which is
A15's line.

**What it was not**, each refuted by measurement rather than by argument:

| hypothesis | the measurement |
|---|---|
| the reset-replay term #737 bounded still scales with age | node, `tab-dilation.mjs`'s shape, mode B's cfg, ages 5 / 30 / 65 s of wall (1.43 / 10.37 / 23.11 s of guest): the reset dance's four control lines answered in ≤ 0.5 ms; the slice carrying the reboot took 9.2 / 2.9 / 3.1 ms; the worst slice after it 74.9 / 21.4 / 18.7 ms; `waiting for download` after 120 / 38 / 33 ms. **Flat, and the oldest board is the fastest.** |
| `putFlash` = `flash-erase` + a 4 MiB `flash-write` starves the inbox | mode B's flash goes through esptool-js, so `putFlash` is not on the path that reproduces at all; and in the instrumented run `persistIfDirty()` never reached 500 ms |
| the OPFS persist (`PERSIST_EVERY_MS`) blocks the thread | same: never 500 ms, in a run whose worst step was 41 445 ms |
| the JIT host's synchronous compiles | `translationEvents` was **0** on every `stats` message of every run |
| `emu_run` itself is slow on this code | 7–8 ms for the 6.4 M-cycle budget, on every long step |
| the tick is starved behind a flood of `usb` messages | `tick` measured 0–1 ms on every long step |
| the PAGE thread was blocked | a 200 ms main-thread heartbeat had no gap ≥ 2 s in any run |

**Fix** — `sample()` computes how long the expired prefix is and drops it in
**one `splice`**. Which samples are kept is unchanged: the same predicate,
applied to the same entries, in the same order — so `dilation()` reports the
same number. Nothing about the pacing rule, the slice, the deficit-drop or
the window's duration moves.

Before and after, the same script on the same machine, one bounded run each:

| | press 1 (blank face, young board) | press 2 (*Update firmware*, board aged 65 s) | worst worker `stats` gap | hub requests ≥ 1 s |
|---|---|---|---|---|
| `origin/main` `77e172c3f` | 30.5 s | **not settled in 120 s** | 39 470 ms | 10 389 ms |
| " | 28.1 s | **not settled in 120 s** | 27 706 ms | 20 226 ms |
| " | 29.2 s | **not settled in 120 s** | 41 445 ms | 9 338 ms |
| " | 28.6 s | 49.8 s | 41 445 ms | 21 246 ms |
| " | 29.2 s | **not settled in 120 s** | 16 484 ms | 13 884 ms |
| with this change | 30.2 s | **30.0 s**, the card reads `Ready` / `fw-esp32c6 77e172c3fb62` | **none ≥ 2 s** | **none ≥ 1 s** |

`just walk-no-board --tab` on the same worktree: **1 pass / 2 failures at
`upload`** on unmodified main, **2 passes of all six steps** with the change.
(Those are three and two observations, not a rate.)

**Regression coverage** — none that is honest. `node scripts/emu/tab-smoke.mjs`
stays green (it does not run the pacing loop — that loop lives in a Worker and
cannot be imported into node, which is why `scripts/emu/tab-dilation.mjs` is a
hand-kept mirror of it). A test for this would have to assert a wall duration,
and no test in this family is allowed to (`lp-emu/esp/README.md`
§Determinism); the falsifiable thing is the *window's length*, and there is no
seam that exposes it. What stands in its place is the paragraph above
`sample()`, which names the number and the shape so the `shift` is not
reintroduced.

**Still open, and deliberately not touched here** — the pacing loop
*busy-spins* whenever the guest is ahead: ~150 000 iterations per second of
wall, each one a `MessageChannel` round trip, a `push`, a `drain()` and two
`emu_*` calls, on a thread whose job at that moment is to wait. The window's
190 000 entries are the symptom that made it visible; the spin itself is the
cause of the volume and it burns a core for nothing. Fixing it means changing
what "ahead: wait" means — a decision about the pacing rule (PD5/D13), not a
bug in it — so it is recorded here rather than legislated in this PR.

**Lesson** — a sliding window bounded in one unit is only bounded if the thing
that feeds it is bounded in that unit too. Here the bound was wall time and the
feed was loop iterations, and the two were held together by nothing but the
incidental fact that a slice used to cost milliseconds; the day the guest could
run *ahead* of the wall — which is what an idle board that fast-forwards `wfi`
does, i.e. the normal state of a board that has been flashed and is waiting for
work — the bound stopped bounding. The second half of the lesson is about the
retirement: `while (…) arr.shift()` is O(n²) and reads as O(n), so a collection
whose length nobody was watching turns a correct trim into a freeze, and the
freeze lands on whoever was waiting on that thread — here a flasher's
`setSignals()`, three layers up, in a file this one has never heard of.
