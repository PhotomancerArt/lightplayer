---
status: carried
since: 2026-08-05      # G1 of the view/edit-split round: visually smooth, ~50ms frames
logged: 2026-08-05
area: lpa-studio-web / lpa-studio-core — dev-profile frame cost at dome scale
related:
  - ../adr/2026-08-05-map2d-editor-selection-tree-model.md
  - ../../lp-fw/fw-esp32v3/probes/flash-layout/README.md   # classic layout noise floor
---
# Dome-scale dev frames sit near 50ms, not 16ms

**Shape** — with 1500 lamps live, the dev-build Studio renders *visually
smooth* but a profile shows ~50ms frames. Accepted at the 2026-08-05 G1
gate ("we can probably punt but long term we're going to need better
solutions"). Dev-build feel is a product signal on this project (the
reference machine is an M2 Max; users run release on weaker hardware),
so this is carried, not closed.

What the 2026-08-05T16:03 trace establishes (even without reliable wasm
symbolization — the binary was rebuilt under the trace):

- The wire-parse whale is gone: per-message post-message cost fell
  ~12.4ms → ~4.2ms after the 857d85a90 opt-level/Rc/memo fixes, and the
  render-side whales (per-frame div/VDOM rebuilds) fell to #347's
  follow-up round (LampView canvas; editor live colors as direct DOM
  writes).
- ~31% of samples are wasm-bindgen-futures task plumbing and ~26% one
  unidentified wasm function. The strongest suspect for the latter is
  the per-tick view-DTO rebuild in `lpa-studio-core`, which is still an
  **opt-level-0 workspace crate** — it is deliberately NOT in the
  `[profile.dev.package.*]` override list because O2 would tax the
  studio iteration loop (it is the most-edited crate). That trade is a
  product-owner call.

## Candidate levers, cheapest first

1. Re-profile with a build that matches the trace (capture + symbolize
   in one sitting) and confirm the 26% suspect before spending anything.
2. `[profile.dev.package.lpa-studio-core] opt-level = 2` (and possibly
   `lpa-client`) — likely the single biggest win if (1) confirms;
   costs studio-core edit-compile time.
3. Thin the per-tick DTO rebuild (dirty-flag or product-keyed reuse so
   an unchanged card rebuilds nothing) — the structural fix, sized like
   a small plan.
4. WebGL/instanced LampView backend — the documented upgrade path;
   keyed to Radiance (~30k) scale, not this.

## Layout noise floor on the classic (2026-09-07)

The device-side counterpart of this entry has a measurement trap of its own.
On the classic ESP32 (DOM-Z-102, `projects/test/zook-dome-1500`), flash code
*placement* alone moves the frame time: the per-core flash cache is 32 KB
two-way set-associative with 32-byte blocks (TRM 1.3.4), shared by IROM and
DROM, and the per-lamp working set — the JIT's scalar-builtin trampolines in
flash, their `libm`/`__divsf3` callees, the sample loop, the direct-lamp
encode and the 2 KB GAMMA16 table in DROM — sat scattered across 1.7 MB of
`.text`, so whether three of its lines shared a cache set depended on how
much unrelated code lay between them. PR #562 saw 50–56 ms from that alone.
Since `hot_text.x` (an ld section-ordering file, `lp-fw/fw-esp32v3/build.rs`)
pins that working set to the head of `.text`, the same source renders the
zook frame in 51–52 ms instead of 56 ms, and a shift sweep — the same LTO
object relinked with 32 B … 12 KB of dead text at an ordered position —
spreads 50–54 ms pinned against 52–60 ms unpinned. **Procedure**:
a before/after `[perf] tick=` claim on the classic must (1) be built with
`hot_text.x` in place and `cache-sets.py` showing every hot function inside
the cluster, and (2) clear ±2 ms, the pinned sweep's spread, or be
run as its own sweep (`probes/flash-layout/shift-sweep.sh`, ten relinks,
minutes on the desk) and compared spread against spread. Details, captures
and the two placements that do not work (a second IROM segment does not
boot; `INSERT` cannot reach `-Tlinkall.x`) are in
`lp-fw/fw-esp32v3/probes/flash-layout/README.md`.

## Exit criteria

Dome-scale dev frames at or under ~16ms on the reference machine, or an
explicit decision that N ms is the accepted dev bar.
