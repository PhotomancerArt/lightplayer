---
status: carried
since: 2026-07-31
logged: 2026-07-31
area: lpc-engine dataflow resolver + lpc-hardware registry (per-frame costs)
related:
  - lp-core/lpc-engine/src/dataflow/resolver/
  - lp-core/lpc-hardware/src/registry/
  - lp-core/lpc-engine/src/nodes/fixture/fixture_node.rs
  - lp-fw/fw-esp32-common/src/output/provider.rs
---
# Frame cost is per-frame resolution machinery, not the shader: ~8.4 ms/fixture flat on the S3

**Shape** — Measured on the desk ESP32-S3 (2026-07-31, all eight node gates,
quad-strips variants, 30-LED strips), then attributed with `lp-cli profile`
(emulator, esp32-c6 cycle model):

| Config | fps | tick |
|---|---|---|
| 4 fixtures + 4 outputs | 20 | 48 ms |
| 1 fixture + 1 output | 50 | 19 ms |
| 1 fixture, render_size 30×1 / 30×8 / 16×16 / **90×90** | 49–51 | **18–19 ms (unchanged)** |
| 1 fixture, **10 vs 120 sample points** | 49–51 | **18 ms (unchanged)** |

The per-fixture cost (~8.4 ms engine-side each) is **flat**: independent of
render resolution (direct sampling never renders the full target) and of LED
count. The emulator profile says where it goes:

- **1-fixture workload**: `[jit] render` (the actual shader) = **1.1 %** of
  self cycles. The frame is dominated by the dataflow resolver re-resolving
  the binding graph from cold every frame — `Resolver::clear_frame_cache`
  runs each tick, so each tick re-walks `resolve → resolve_binding_source →
  resolve …` (19 KB deep stacks), with `SlotPath::parse` **per frame**,
  `String::clone` (2.8 %), `QueryKey` alloc/eq/drop, `slot_lookup` (2.5 %),
  and the allocator+memcpy pair at **~46 %** of self cycles combined.
- **4-fixture workload** adds the second mechanism:
  `HwRegistry::endpoint_status_for` = **45.8 %** of all cycles
  (`VirtualWs281xDriver::endpoints`, `endpoint_for_spec`, `validate_spec`,
  and ~5 % of `core::fmt` behind it) — per-frame re-enumeration of hardware
  endpoints with per-endpoint status recomputation and string spec
  formatting, scaling with output channels. (Profile uses the virtual
  driver; the enumeration seam is shared engine/registry code, not
  emulator-only.)
- Blocking RMT sends: 1.3 ms/channel (measured on silicon) — 11 % of the
  4-fixture frame. Real, but a rounding error next to the above.

Verdicts this kills: the LED ledger's "sends are serialized" suspicion
(P4, exonerated), and "the Xtensa JIT is slow" — codegen is ~1 % of the
frame; per-clock comparison with the C6 is resolver-bound on both chips.

Profiles: `profiles/2026-07-31T18-02-07--…-1fix--steady-render--s3-gate-perf-1fix/`
and `…18-02-44--…quad-strips--steady-render--s3-gate-perf-4fix/` (report.txt).

**Carrying cost** — Multi-fixture projects degrade linearly (~8.4 ms per
fixture+output chain): 4 fixtures = 20 fps today; ~10 fixtures ≈ single-digit
fps, regardless of how small the fixtures are. Authors cannot buy the cost
down with lower resolution or fewer LEDs, which makes the scaling feel
arbitrary from the outside.

**Workarounds** — Fewer fixture+output chains (one fixture spanning strips);
nothing else helps, by measurement.

**Incident log**
- **2026-07-31** — Filed from the S3 node-gates plan P4 with an initial
  (wrong) "render/sampling dominates" attribution; corrected the same day at
  the gate after Yona pushed back on 20 fps: resolution/LED sweeps showed the
  cost is flat per fixture, and the emulator profile convicted per-frame
  resolution machinery (dataflow resolver + endpoint status) instead. The
  suspicious shapes: `clear_frame_cache` discarding all resolution work every
  tick, and endpoint status recomputed per frame per channel.

**Exit criteria** — A profiled optimization pass that makes resolved bindings
and endpoint status persist across frames (invalidate on tree/binding/
hardware change, not per tick), after which frame cost is dominated by actual
rendering work and the profile's top self-cycle entries are no longer
allocator/memcpy/string machinery. Re-measure the same quad-strips matrix on
the desk S3 as the oracle.
