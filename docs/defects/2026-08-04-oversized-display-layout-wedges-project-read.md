---
status: fixed
found: 2026-08-04      # how: first Studio open of examples/zook-dome (1500 lamps)
fixed: pending         # commit updated at land time
area: lpc-engine control-product probe + lpc-shared project-read transport
class: unbounded-payload-on-bounded-transport
related:
  - none
---
# A dome-scale display layout wedges the whole project read stream

**Symptom** — opening `examples/zook-dome` (1500 lamps, one fixture) in
Studio showed a red project banner:

```
protocol error: project read failed: Serialization error:
project-read event exceeded frame budget of 16384 bytes
```

and the workspace never synced — "project sync failed" repeating, module
face stuck on "Tracking product", panel and faces dead. The simulator
itself ran fine (the runtime path doesn't ride project-read), and the
gallery's live thumbnail rendered, which made the wedge look arbitrary.

**Root cause** — the control-product probe ships the fixture's
`ControlDisplayLayout::Layout2d` — one JSON entry per lamp — inside a
single project-read event. The transport's stream sink
(`lpc-shared/src/transport/server.rs::push_event`) chunks *batches* to
`PROJECT_READ_FRAME_MAX_BYTES` (16 KiB) and streams bulk sample bytes in
bounded chunks, but a single event larger than the budget is a terminal
stream error by design ("project-read event exceeded frame budget").
A 241-lamp layout fits with room to spare — the size regime every
fixture before this one lived in. 1500 lamps serialize to ~100 KiB+:
the first dome-scale fixture opened in Studio turned the deliberate
single-event backstop into a whole-project wedge. The gap was known in
outline: `lpc-wire`'s probe tests note the unchunked-header growth path
as the deferred "semantic layout split" escalation.

**Fix** — `control_display_layout_result` (lpc-engine `engine.rs`)
measures the layout with the wire serializer's counting sink before
attaching it, and over `DISPLAY_LAYOUT_WIRE_BUDGET`
(`PROJECT_READ_FRAME_MAX_BYTES` − 2 KiB header/envelope margin) returns
`ControlDisplayLayoutProbeResult::Unsupported` with the measured size in
the reason. Clients already render `Unsupported` as a layout fallback,
so the project view, panel, output face, and native sample preview all
work; only the lamp-dot display degrades. The guard sits in the shared
engine, so a device serving Studio over USB is protected identically.
Regression: `fixture_project_read_refuses_over_budget_display_layout`
(fixture_node.rs).

**Follow-up** — the real capability, streaming large layouts in bounded
chunks (the "semantic layout split"), is now motivated by a concrete
product surface: dome-scale fixtures deserve lamp dots in the fixture
face too. It needs its own plan: chunked layout events in `lpc-wire`,
producer-side splitting, client reassembly, and device memory bounds.
Until then the standalone mapping editor (`#/mapping`) is the full-size
mapping view — it loads the document locally and never rides the probe.
