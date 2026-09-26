---
status: fixed
found: 2026-09-25      # report (Yona, reviewing PR #827 on an emulated C6) + live-debugging
fixed: 0456f590f (PR #836)
area: fw-esp32c6 serial/io_task.rs
class: wake-quantum-throttle
related: [docs/adr/2026-09-24-json-pack-wire-encoding.md, "planning lp2025/2026-09-25-1410-studio-device-sluggishness"]
---
# The C6 takes a request in at one USB packet per frame

**Symptom** — Studio felt sluggish against a device, worst on the emulated
C6 (Yona, 2026-09-25, reviewing #827's Play-mode pattern picker). Every
control waited on round trips, and each round trip was slow. The JSON Pack
ADR had already seen the shape without naming it: a lens pause of 150 / 75 /
33 ms moved reads per second only from 2.3 to 3.5, because "the read's
service time on the board dominates".

**Root cause** — `read_serial` read one 64-byte OUT packet (the endpoint's
whole buffer) per call, and `io_task` shares the one cooperative executor
with the server loop, which yields for 1 ms per frame. So the task got about
one read per frame, and the board took requests in at 64 B per frame. A
Studio lens request is ~600 B of JSON (host→board is never packed), so it
spent ~10 frames arriving before the server saw it. Measured on the emulated
C6 (`lp-emu:esp32c6:t1`, #827 head, `playful-choker-tryout`): round trip is
linear in request bytes at 0.37 ms/B of wall = 0.128 guest ms/B = 64 B per
9.1 guest-ms frame. The emulator (0.35–0.45× real time) multiplies this; it
does not cause it. The mechanism is firmware-side, so a real C6 shows it too.

**Fix** — `read_serial` drains the burst: after the first packet it keeps
reading while the next one follows within 500 µs (`READ_BURST_GAP`), up to
4 KB per pass (`READ_BURST_MAX`). Same rig, same project, emulated C6,
median of 15:

| request | before | after |
|---|---:|---:|
| 41 B `listLoadedProjects` | 41 ms | 33 ms |
| 283 B gated lens read | 140 ms | 55 ms |
| 600 B lens read | 354 ms | 92 ms |
| 1,200 B | 402 ms | 133 ms |
| 2,400 B | 764 ms | 219 ms |

**Confirmed** — Yona, 2026-09-26: Studio against both the emulated C6 and a
real C6 with #836 and #827 merged is "much better"; the feel check passed.

**Regression coverage** — none automated yet: the rig
(`rtt_rig.py` in the planning dir's `measurements/`) is a desk tool, and
emulated wall time must not be gated. A guest-time version (request bytes →
guest ms to first reply byte) is the candidate for a chip test.

**Lesson** — a read loop's quantum per wake is a throughput cap, and on a
cooperative executor the wake rate is the frame rate. fw-esp32s3's
`io_task` had the identical one-packet `read_serial` and has since been
ported to the same burst drain (`READ_BURST_GAP` / `READ_BURST_MAX`), by
analogy to the C6 measurement above — there is no S3 emulator boot of the
shipped image to measure it directly. A remaining ~0.07 ms/B slope
after the fix, and tick-before-messages (a request waits one full render
after it lands), are the next firmware-side costs.
