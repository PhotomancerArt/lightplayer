---
status: fixed
found: 2026-08-28      # how: small-dome full-scale example stress test (browser sim lamp preview)
fixed: 2026-08-28      # same PR (#460): EngineLinkState threaded through the wire-load handler
area: lpa-server (wire load handler) + lpc-engine (display-layout budget)
class: state-conflation
related:
  - ../use-cases/2026-08-28-three-domes.md
  - 2026-08-14-sibling-module-bus-tie-blanks-preview.md
---
# Wire-loaded projects kept the fail-safe serial budget on unbounded links

**Symptom** — In the browser sim, the full-scale small-dome example
(5,950 dome lamps + 360 door lamps) drew ONLY the door in every lamp
preview (module face, simulator pane), while the output cards proved the
control wire fully alive. No error surfaced anywhere: the engine's
display-layout refusal is deliberately in-band and per-producer, and the
published-frame merge silently drops a producer whose layout probe
refuses — the composite just misses those lamps.

**Root cause** — Two load paths, one dressing. `LpServer::load_project`
(the host-call path) stamps the link's engine state — display-layout
budget, safe output clamp — onto the freshly created engine ("every
engine wears them"). The wire path (`ClientRequest::LoadProject` →
`handlers::handle_load_project`) called `project_manager.load_project`
directly and skipped the stamping, so a wire-loaded engine kept the
engine's fail-safe DEFAULT budget: the 16 KiB serial frame. On a link
that had declared itself unbounded (`set_project_read_frame_budget(None)`
— the browser sim's postMessage transport, fw-browser `runtime.rs`), any
fixture whose packed layout exceeds ~14 KiB (~2,650 lamps) was refused as
`Unsupported` and dropped from the merged output geometry. The browser
sim loads every project over the wire, so the sim NEVER showed a
dome-scale fixture's lamps. Zook (1,500 lamps) fit under the ceiling,
which is why the gap survived until a >2,650-lamp fixture existed.

**Fix** — `handlers::EngineLinkState` (budget + clamp, `Default` = the
fail-safe serial posture) is passed by the dispatch site from the
server's declared link state and applied to the fresh engine inside
`handle_load_project`, mirroring the host-call path. Regression:
`lpa-server/tests/wire_load_link_state.rs` (a wire re-load wears the
unbounded budget; the default keeps the serial one). Diagnosis pinned by
the engine-side probe: at unbounded budget both small-dome outputs answer
full layouts (3,335 + 2,975 lamps); at the serial budget out_a degrades
to the door's 360 and out_b refuses at 32,357 bytes.

**Watch for** — the per-producer refusal path is still silent by design
(report-in-band, never a dead frame): a future budget regression will
again look like "some fixtures don't draw," not like an error. The
2,048-lamp ceiling remains the declared posture for real serial links.

**Remaining symptom (resolved 2026-08-29, separate mechanism)** — with
this fix the browser sim drew out_a completely (25 panels + door, 3,335
lamps), but out_b's lamp geometry still never reached the composed lamp
view, even though the engine answers its full 2,975-lamp layout at
unbounded budget (verified by direct probe). The drop was client-side:
every lamp compositor (device/sim card feed, preview-host feed,
module-face hero) reduced the probe's per-output answer to ONE output.
Filed and fixed as
[lamp-views-latch-one-output](2026-08-29-lamp-views-latch-one-output.md).
