---
status: open
found: 2026-10-03      # how: hardware-walk — PR #942's G-S3 desk sitting (agent-run), silicon vs the PR's emulated table
area: lp-emu/esp/lp-emu-esp32s3 time grade t1 (one cycle per instruction, no cache / flash-wait / per-class model; the machine has no other grade)
class: fidelity
related:
  - docs/defects/2026-10-01-the-emulated-c6-charges-a-cold-code-path-10x-less-than-silicon.md
  - docs/adr/2026-10-02-c6-link-io-thread.md
  - lp2025/2026-10-02-1918-io-thread-other-boards
---
# The emulated S3 at t1 shows no frame-rate cost for link load; silicon loses about a fifth of its frame rate while requests are answered

**Symptom** — PR #942 (the S3's link IO on its own core-0 thread,
messages-first) was measured on `lp-emu:esp32s3:t1` before any board was
touched, then on the desk S3 (`D8:3B:DA:47:29:70`, wall clock) with the same
`lp-cli link rtt` and the same `projects/test/shader-oracle`. The request
latency, counted in frames, carries over to within about 0.1 frame. The frame
rate under link load does not carry over at all:

| | `lp-emu:esp32s3:t1`, lp-emu `ab8345d38` (scaled rig, PR #942 body) | silicon, main `9f70f39da` / branch `20e9b64e5` |
|---|---|---|
| idle fps, main → branch | 239.41 → 239.16 | 54.52, 55.59 → 55.12, 53.97, 55.17 |
| fps while requests are answered, **main** | 240.1 (+0.3 %) | 50.6, 50.4 (**−7 %, −9 %**) |
| fps while requests are answered, **branch** | 239.7 (+0.2 %) | 44.6, 44.4 (**−19 %, −20 %**) |
| fps during transfers, branch | 217.1 (−9 %) | well under half the idle rate (estimate: the transfers last ~1.2 s inside a 5 s heartbeat window) |
| request RTT p50 / p90, branch | 0.72 / 1.14 frame | 0.79–0.87 / 1.18–1.23 frame |
| request RTT p50, main | 1.56 frame | 1.67, 1.73 frame |
| main's transfers, per frame (frame-bound: the link task shares the render's executor) | ~430 B ↓ / ~505 B ↑ a frame (100.3 / 118.0 KiB/s at 239 fps) | ~252 B ↓ / ~262 B ↑ a frame (13.4–14.4 / 13.9 KiB/s at ~55 fps) |

Silicon "while requests are answered" is the one 5 s heartbeat window that
lies wholly inside `link rtt`'s request phase (150 `ListLoadedProjects`, one at a
time, each after a random 0–40 ms pause: ~26 requests/s on the branch, ~18/s
on main, because the branch answers twice as fast). Every silicon run is in
`s3-*.json` beside the PR #942 desk report; the emulated rig was scaled
(warm-up 2 s, no idle phase, 100 requests) because the S3 emulator runs ~50×
slower than board time.

So on silicon, answering a request costs the render several milliseconds of
core-0 time (roughly 5 ms a request on main and 7.5 ms on the branch, read
off the frame deficit per request), and the emulator charges about nothing.
A walk or a `link rtt` run on the emulator cannot show a frame-rate
regression caused by link traffic on this chip — the same blind spot the C6
has (2026-10-01 entry), now on the S3, which has **no** grade beyond `t1`.

**Root cause** — not established; two facts bound it. (1) `t1` counts one
cycle per instruction with no cache, flash wait-state or per-class cost
(`validate.toml`'s `lp-emu:esp32s3:t1` time row says so and grades nothing
on it), and silicon renders `shader-oracle` 4.3× slower than `t1` does
(55 vs 239 fps) — the render runs from flash through the S3's cache. (2) On
the C6 the same shape (a preempting link thread whose cost the emulator does
not see) was traced to `t1`/`t2` having no memory-cost model, and `t3`'s
measured cache fills reproduce silicon there. The likely mechanism here is
the same — the request path and the link thread run cold out of flash after
every render — but nobody has measured an S3 cache fill or run a
`cycle-probe` kernel on this chip, so it stays a candidate.

Not explained by `t1` alone, and recorded so it is not lost: main's
frame-bound transfer moves 1.7–1.9× more bytes **per frame** emulated than on
silicon, and main's link RTT in the request phase is 1.26 frames emulated
against 0.68–0.80 frame on silicon. Both point at the host side of the
USB-Serial-JTAG model (when the emulated host drains, against a macOS host's
real USB scheduling) as much as at the CPU clock.

**Fix** — none yet. The C6's path is the template: a `cycle-probe` arm for
the S3 (`board/esp32s3/cycle_counter.rs` already reads `ccount`), a measured
cache-fill cost, and a cache-charging grade for `lp-emu-esp32s3`; then
re-run PR #942's rig at that grade and see whether the request-phase frame
deficit appears.

**Regression coverage** — none: the S3 has no time grade that could carry
an assertion, and nothing gates on emulated time by rule.

**Lesson** — on the S3, "idle fps within 2 %" and request latency in frames
are emulator-checkable; frame rate **under link load** is not, on either
side of a threading change. Until the S3 has a cache-charging grade, any
claim about what link traffic costs the render on this chip needs a desk
number. Studio's own polling (the editor lens reads every 150 ms, ~7
requests/s) is in the range where the silicon cost is visible.
