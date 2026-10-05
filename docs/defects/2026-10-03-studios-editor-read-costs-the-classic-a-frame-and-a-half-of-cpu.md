---
status: open
found: 2026-10-03      # hardware-walk (PR #943's desk A/B, agent-run, real Studio)
area: lpa-server project read (editor lens) × fw-esp32v3 (classic ESP32)
class: budget-exhaustion
related:
  - 2026-10-03-the-emulated-classic-renders-14x-faster-than-silicon-and-hides-the-link-threads-frame-rate-effects
  - 2026-10-03-the-emulated-s3-shows-no-frame-rate-cost-for-link-load
  - docs/adr/2026-10-02-c6-link-io-thread.md (the classic amendment)
---
# Studio's editor read costs the classic a frame and a half of CPU, so its LEDs hitch whenever the editor is open

**Symptom** — Yona, 2026-10-03, on the classic (DOM-Z-102, 4 outputs): with
Studio connected the LEDs themselves go visibly choppy, not just Studio's
view. Measured on the same board with an on-board frame-timing diagnostic
(`frame_pace_diag`: every frame timestamped as it reaches the RMT driver) and
a real Studio (release build, headless, editor open on the board, recorded
with `?record=`), steady state, per 5 s window, a ~16 fps project:

| firmware | slow frames (> 1.5× median) | their length | judder p90 | server tick, answering a read / idle |
|---|---|---|---|---|
| main `589b893f5` | 13 of ~47 | ~223 ms | ~3 ms | 217 ms / 61 ms |
| PR #943 as first built (thread + messages-first) | 15 of ~39 | ~240 ms | **181 ms** | 234 ms / 61 ms |
| PR #943, link thread off | 10–11 of ~60 | ~198 ms | ~3 ms | 193 ms / 56 ms |

With the editor closed (Devices page only) a read costs the tick ~7 ms and
nothing hitches. So the hitch is the editor's read, on every build; the link
arrangement only moves it.

**Root cause** — the editor's lens read (`projectRead`, ~2.6–3 a second
while the editor is open: a 150 ms pause after each answer) costs the
classic **~140–175 ms of CPU inside the server tick**, more than two of
this project's frames. The server answers a tick's requests in the same
task that renders, so the frame after a read is late by that much. What one
read asks for (decoded off the recorded wire):

```json
{"projectRead":{"handle":1,"request":{"since":692,
  "queries":[{"shapes":{"level":"detail"}},
             {"nodes":{"level":"detail","nodes":"all","include_slots":true}},
             {"runtime":null}],
  "probes":[{"output_frame":{"geometry":{"if_changed":…},"samples":"srgb8"}},
            {"binding_graph":{"structure":{"if_changed":…},"include_values":true}}]}}}
```

answered with ~680 B packed. Which part costs what is **not yet measured**
(next step: time each query and probe on the board). Two things ride every
read that should not:

- the `runtime` query calls the chip's memory-stats hook, and the classic's
  hook (`fw-esp32v3` `esp32_memory_stats`) *logs* — `[MEM] …` and `[JIT] …`,
  two log records per read (~5–6 a second through the log ring and onto the
  link), and scans the main stack (`stack_probe::log_if_grown`) — all of
  which its own comment sizes for "the heartbeat cadence";
- on `main` `589b893f5`, every read also flipped the shader node's
  bound-input status: `[visual-shader-node] bound inputs failed to resolve
  (node=NodeId(5)): input "phase" using its default: produce: t…` then
  `bound inputs resolve again` (two more log records, and evidence that the
  read evaluates the node's bindings outside the frame — `binding_graph`'s
  `include_values`, by the look of it). Not seen on `origin/main` at
  `626a1b851`.

The JIT is **not** re-run: `[JIT] allocs` stays 1 across thousands of reads.

**Why messages-first made it worse** — answered *before* the render, a read's
~175 ms lands between the moment a frame takes its clock and the moment it
reaches the LEDs, so the picture shows a time ~180 ms stale and the next one
jumps: judder p90 181 ms, where render-first keeps it ~3 ms. That is why the
classic keeps messages-first off with its link thread (PR #943; the ADR's
classic amendment).

**The other chips (estimate, not measured)** — the read is the same request
everywhere and its cost is CPU, so the C6 and S3 pay a similar order of
cycles per read. Both render their projects faster and have faster cores, so
the stall is a smaller share of a frame and shows as a shorter hitch: Yona
found the S3 "felt better" (one output against the classic's four). PR #942's
own desk sitting measured the S3's frame rate falling 7–9 % (main) and
19–20 % (link thread + messages-first) under `link rtt`'s cheap requests,
which is the same family of cost. `frame_pace_diag` measures it on any chip
that calls `frame_emitted()` from its output driver; only the classic does
today.

**Fix** — open. Directions: make the editor's read cheap on the board (send
only what changed, drop `include_values`/`detail` from the steady-state poll,
move the stats out of the read), take the memory-stats logging off the
per-read path, and/or let the server spread a read's work across frames.
None is in PR #943.

**Regression coverage** — none yet. The emulator cannot show it (no flash
cache model, 14× fast; see the related fidelity entry); a silicon number from
`frame_pace_diag` under a recorded Studio session is the measure.

**Lesson** — "choppy while connected" was read as a link problem because the
link was what had just changed. The on-board frame timing says the host's
*requests* cost more than the link that carries them; measure the frame, not
the transport, before tuning the transport.
