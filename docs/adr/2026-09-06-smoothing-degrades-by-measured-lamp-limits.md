# ADR: Display-pipeline smoothing degrades above measured lamp-count limits on the board manifest

- **Status:** Accepted
- **Date:** 2026-09-06
- **Deciders:** Photomancer
- **Plan:** `lp2025/2026-09-06-0047-scale-dependent-smoothing` (PR #524)
- **Supersedes:** None
- **Superseded by:** None
- **Related:** [2026-08-05-manifest-soft-limits-are-measured-records.md](2026-08-05-manifest-soft-limits-are-measured-records.md)
  (the record shape this extends),
  [2026-09-02-fault-is-never-black.md](2026-09-02-fault-is-never-black.md)
  (the honesty lineage the status follows),
  [2026-08-03-memory-pressure-at-compile-safe-points.md](2026-08-03-memory-pressure-at-compile-safe-points.md)
  (why a pressure-driven decision was rejected)

## Context

Every authored output defaults `interpolation_enabled` and
`dithering_enabled` to ON (`default_true_slot()` in
`lpc-model/src/nodes/output/output_def.rs`; `DisplayPipelineOptions::default`
agrees). On the ESP32 providers, `DisplayPipeline::new` then holds `prev` +
`next` (12 B/lamp) for interpolation and a dither carry (3 B/lamp), per
port, on top of the 6 B/lamp `current` it always holds. The per-lamp memory
table (`docs/reports/2026-09-02-per-lamp-memory-table.md`, after PR #503)
left this as the largest remaining per-lamp line on the classic: 1,500 B at
100 lamps, 22,500 B at zook's 1,500 against a 186,368 B arena — and the only
per-lamp line that is a quality feature rather than a home for data.

Yona's direction: "it's fine at 100 LEDs." Keep smoothing on small
fixtures; degrade at scale on memory-limited boards.

Three ways to get there were on the table:

1. **Change the authored defaults** to off. Punishes every small fixture
   (the common case), and a change to a persisted default is a format
   change with a migration story.
2. **Decide at `open` from the heap** (turn interpolation off when free
   memory is below some watermark). Non-deterministic: the same project
   renders differently on two boots depending on what else happened to be
   resident. The compile-window drops
   (`docs/defects/2026-08-29-shader-jit-compile-transient-starves-classic-heap.md`)
   are the standing precedent against pressure-driven rendering changes.
3. **Decide at `open` from the board's measured limits and the lamps
   open.** Board-side, scale-dependent, deterministic, and absent on every
   board that has not measured it.

## Decision

### 1. Two more soft-limit records

`HwSoftLimits` (`lpc-hardware/src/manifest/hw_soft_limits.rs`) gains
`interpolation_leds` (`interpolationLeds` on the file) and `dithering_leds`
(`ditheringLeds`), each an `HwMeasuredLimit { value, measured }` like
`totalLeds`. Optional, serde-defaulted, `skip_serializing_if` absent —
older manifests parse unchanged, older firmware ignores the fields, no
format bump (the same additive posture ADR 2026-08-05 chose). The schema is
regenerated.

Rule, per feature: **total lamps open across the board `>` the record's
value ⇒ the feature is opened OFF.** Interpolation first (4× the bytes and
a second `write_frame` per tick); dithering has its own, higher record. A
board without a record never degrades that feature. The provider only
ever turns a feature OFF relative to what was authored — an author-disabled
feature is never turned on, and is not "reduced".

### 2. The tier is a function of the OPEN SET

Ports open one at a time (`EngineServices::ensure_port_open`), so the
board total is only known incrementally. A per-open decision on the
running total would leave the first ports smoothed and the later ones not,
with the final state depending on open order. Instead
`Esp32OutputProvider` (`fw-esp32-common/src/output/provider.rs`) keeps
each port's authored options, and on **every open and every close**
recomputes the total and rebuilds any port whose pipeline disagrees with
the tier the total selects. A close that drops the total back under a
limit restores the feature. Same set of ports ⇒ same pipelines, in any
order.

Allocation discipline, because memory is the point: a downgrade frees the
old pipeline before allocating the smaller one (through a one-lamp
stand-in), so the transient never holds both; an upgrade allocates first
and swaps only on success — a heap that cannot afford `prev` + `next` back
keeps the reduced pipeline and keeps saying so. Re-tiering never runs from
the write path; the write path's own resize keeps the port's current tier
until the next open or close.

### 3. The downgrade is honest, and quiet

`OutputProvider::port_smoothing(handle) -> Option<OutputPortSmoothing>`
(`lpc-shared/src/output/output_port_smoothing.rs`; defaulted to `None`, so
the host and emulator providers change nothing) reports what was taken:
the board total and the limit(s) exceeded. The engine samples it once per
flush per wire (`OutputWire::smoothing`), folds it per node
(`EngineServices::output_smoothing_notice`), hands it to the output's
consume context (`TickContext::with_output_smoothing`, the
`with_project_fault` precedent), and `OutputNode::runtime_status()` wears
it as the **lowest-priority** status:

```
Warn: smoothing reduced at scale: 1500 lamps open on this board — interpolation off (limit 500), dithering off (limit 1000)
```

`Warn`, not `Fault`: the output is doing exactly what the board's measured
limits say and nothing needs fixing (ADR 2026-09-02's table — "degraded
but still rendering"). It rides the existing `NodeRuntimeStatus::Warn(String)`
variant, so the wire is unchanged. It is logged at the provider once per
open/close transition, never per frame.

### 4. DOM-Z-102's first values are DERIVED, and say so

`interpolationLeds = 500`, `ditheringLeds = 1000`. Provenance in the
manifest states plainly that these are derived from the per-lamp memory
table's measured slopes and the "fine at 100 LEDs" floor — not from a soak
at the limit. Under ADR 2026-08-05 a record is falsifiable: a bench `[mem]`
bracket replaces value and provenance together. Every other board carries
no record and never degrades.

## Consequences

- Zook on the classic: per-port pipelines go from 21 B/lamp to 6
  (−22,500 B at 1,500 lamps); fixtures of ≤ 500 lamps keep both features.
  Proven on the host by `option_gated_buffers_cost_exactly_their_documented_bytes_per_lamp`
  (the allocation) and `zook_shaped_totals_turn_both_features_off_on_every_port`
  (the tiering).
- The board's manifest, not the project, decides the trade — the same
  project on a board without records renders exactly as authored.
- A quality difference between boards is now visible in Studio as an
  output badge rather than discovered by eye on the wall.
- The device card does not aggregate node statuses (the heartbeat mirror
  carries none — ADR 2026-09-02), so the badge lives on the output node
  only. Reaching the card would be a wire change; deferred.
- Provider `open`/`close` now do a little more (a sum over ≤ 8 ports and,
  on a crossing, a rebuild); the write path is untouched.

## Alternatives Considered

- **Authored defaults off** — rejected (§Context 1): punishes small
  fixtures, and a persisted-default change.
- **Heap-driven at open** — rejected (§Context 2): non-deterministic,
  against the compile-window precedent.
- **Per-port lamp count instead of board total** — a 5 × 300 dome never
  crosses a per-port limit that a single 1,024-lamp wire does; the heap is
  spent by the board total, so the limit is the board total.
- **Decide once on the running total, never re-tier** — order-dependent
  and inconsistent across ports of one fixture; rejected in favour of §2.
- **Carry the notice on `OutputWireStatus` / the hello** — a wire change;
  the `Warn` string already reaches the badge.

## Follow-ups

- Replace the derived DOM-Z-102 provenance with a bench `[mem]` bracket at
  the limits (`docs/adr/README.md`, deferred decisions).
- Device-card aggregation of node `Warn`s once the heartbeat mirror
  carries statuses.
- S3 / C6 manifests may carry their own records once measured.
