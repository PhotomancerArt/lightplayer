# ADR: Display layouts cross the wire packed, against a transport-declared budget

- **Status:** Accepted
- **Date:** 2026-08-10
- **Deciders:** Photomancer
- **Supersedes:** the "semantic layout split" escalation recorded in
  `2026-07-04-envelope-streaming` (for the display-layout payload only)
- **Superseded by:** None

## Context

A control product's display layout — where to draw each lamp — used to
serialize as one JSON tuple per lamp (`[lamp_index, sample_start, x, y,
radius]`), roughly 75 bytes a lamp. A single project-read event larger than
the 16 KiB frame budget is a terminal stream error, so the engine refused
dome-scale layouts as `Unsupported`
(`docs/defects/2026-08-04-oversized-display-layout-wedges-project-read.md`),
and Studio grew a client-side fallback that re-derived the geometry from
package files and mapping documents.

That fallback then drifted. The engine's layouts became placement-aware —
an output's geometry is N producers' layouts rebased through the fragments
that placed them (`2026-08-10-output-fragments-and-patch-files`: fixture-
space geometry never crosses the wire boundary un-rebased) — while the
fallback kept asking ONE package fixture and numbering lamps `channel * 3`.
For a patched multi-fixture wire (`examples/peach-2d`) it drew the body's
lamps from the leaf's bytes and the leaf nowhere at all. A mirror of engine
logic maintained by hand is a standing invitation for exactly this class of
defect.

Meanwhile the budget itself was a lie on most links: 16 KiB is physically
real only on the ESP32 serial writers (stack-sized buffers). Websocket,
in-proc host, and the browser sim have no such limit, yet paid the same
dome-scale refusal because `DISPLAY_LAYOUT_WIRE_BUDGET` was an engine
constant.

Measurement (planning study, reproduced in the A1 test): today's JSON at
~75 B/lamp; packed at ~5.4 B/lamp; zlib over the JSON does NOT rescue the
dome (24.5 KiB, still over); zlib over the packed form gains ~nothing on
irregular geometry (a quantized spiral is near max entropy) and ~50× on
regular grids (a serpentine grid's deltas are constant). Device-side
compression also fights the 0 %-margin heap ratchet and the classic's
two-region heap (no contiguous 32 KiB deflate window to spare).

## Decision

1. **Packed wire encoding.** `ControlLayout2d` serializes ONLY as packing
   spans plus quantized centers: `"s"` carries maximal runs as
   `[first_lamp, count, sample_start, sample_stride, radius]` 5-tuples
   (split wherever an index, stride, or radius invariant breaks — packing
   is total, worst case one span per lamp); `"c"` carries base64 u16le
   `(x, y)` pairs, one per lamp, quantized against the normalized extent
   (1/65535 grid, far below any real lamp radius — the one lossy field).
   The semantic `"p"` path-span list stays separate from the packing spans
   so "paths unknown" survives a round trip. wireProto 17 → 18, lockstep,
   old form deleted (AGENTS.md wire posture).
2. **A1 — 2048 lamps is the embedded ceiling.** The declared product
   posture for esp32-class links, pinned by
   `a_2048_lamp_layout_fits_the_serial_frame_budget` (lpc-wire) over
   max-entropy scattered centers: ~11.5 KiB against the 14.3 KiB layout
   budget. Beyond it a serial link answers `Unsupported`, permanently for
   the connection, and the UI says so honestly (the device card names the
   refusal instead of painting a blank canvas).
3. **Transport-declared budget.** `LpServer::set_project_read_frame_budget`
   is the one knob, defaulting to the embedded serial frame (fail-safe for
   un-plumbed hosts). From it derive BOTH the engines' display-layout
   budget (frame − `PROJECT_READ_PROBE_HEADER_RESERVE_BYTES`) and the
   stream sink's per-event ceiling (declared frame, or a 4 MiB runaway
   ceiling when `None`), so the two can never disagree. Plumbing mirrors
   `safe_output_clamp`. In-proc host, browser sim, and `lp-cli serve`
   declare `None`; `FakeEsp32Device` keeps the serial default because it
   emulates a serial device. Radiance-scale (~30 k lamps) geometry never
   rides the serial frame — it rides unbounded links, where the engine now
   answers.
4. **Header totals are budgeted too.** The output-frame probe header
   carries every output's layout in one unchunked event; the engine now
   accounts a running header total and degrades later entries to
   `Unsupported` once it is spent — three individually-legal layouts can
   no longer jointly wedge the read stream.
5. **The client-side synthesis fallback is deleted.** Both paths (lens and
   device-card package-file synthesis), their caches, and the never-called
   preview-host synthesis API. Geometry comes from the control product or
   it is honestly absent. The `layout_refused` stop-asking latches stay:
   refusal is now genuinely permanent, and re-asking would re-build and
   re-measure a layout the engine will refuse again.

## Consequences

- The dome (1500 lamps) draws from the engine's own placement-aware merge
  on every link, including embedded serial — the drifted-mirror defect
  class is structurally gone.
- Lamp centers are quantized on the wire (≤ 1/65535 absolute error).
  Integer fields — indexes, sample offsets, spans — round-trip exactly.
- A wire peer older than proto 18 is refused outright (reflash), per the
  heavy-development wire posture.
- The engine default keeps un-plumbed embedders serial-safe; only hosts
  that KNOW their pipe is big opt out.
- `2026-08-04`'s defect closes: the wedge is prevented by the sink budget
  pairing, the refusal path, and the header-total accounting, and the
  degradation the refusal buys is now rare (>2048 lamps on serial only).

## Alternatives Considered

- **Semantic per-lamp-range split (chunked layout streaming)** — the
  escalation `2026-07-04-envelope-streaming` prescribed. Rejected for this
  payload: packing reaches the declared 2048-lamp ceiling in ONE frame,
  chunking adds reassembly state on both ends for a regime (>2048 on
  serial) ruled out of product scope, and radiance-scale hardware talks
  over links with no 16 KiB constraint at all.
- **Compression (deflate/LZ on the payload)** — measured, not argued.
  No material gain on irregular geometry after quantization; the only big
  win (regular grids, ~50× with delta+deflate) serves installs beyond the
  declared embedded ceiling; the compressor would live on the chip where
  the heap ratchet holds a 0 % margin. Studio already links a deflate
  decoder (zip), so this stays cheap to revisit if the trigger fires.
- **Raising `PROJECT_READ_FRAME_MAX_BYTES`** — the frame sizes a stack
  buffer on the C6/classic; growing it spends scarce DRAM on every device
  for a preview payload.
- **Keeping the client fallback, made fragment-aware** — a sibling branch
  implemented the refuse-when-ambiguous half (a2d3308c9, superseded). Any
  client re-derivation re-creates the drift class this ADR exists to end.

## Follow-ups

- **>2048-lamp single outputs on a serial link** (a real install, not a
  synthetic): revisit as semantic span shapes — grids/rings described by
  their generator rather than per-lamp lists (the authored map2d document
  is the low-entropy form) — or the per-lamp-range split, whichever the
  case warrants. Until then, honest `Unsupported`.
- **Wire compression generally**: only if project-read frames become
  byte-bound in practice; the probe-performance notes ranked the 750 ms
  refresh interval and the 10 ms receive poll far above encoding overhead.
- The informational hello field for the declared frame budget
  (`HardwareFacts`, additive) was skipped — clients have no consumer for
  it today.
