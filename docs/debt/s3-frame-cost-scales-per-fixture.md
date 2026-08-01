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
(emulator, esp32-c6 cycle model). The table below is the ORIGINAL measurement;
see the 2026-08-01 incident-log entry for the post-resolver numbers:

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
- **2026-07-31** — **Resolver half addressed** (PR #243,
  `docs/adr/2026-07-31-resolver-persistent-resolution.md`). Routes, binding
  literals and authored-def reads now persist across frames and are dropped
  on structural change instead of per tick. Measured on the 1-fixture
  workload (`projects/test/quad-strips-1fix`, committed as the reproducible
  oracle), steady render, esp32c6 cycle model:

  | | Before | After |
  |---|---|---|
  | Total attributed cycles | 2,468,427 | 1,146,110 (−54%) |
  | allocator + memcpy | 44.0% | 34.4% |
  | `[jit] render` | 1.1% | 2.4% (same cycles) |

  `QueryKey::eq`, `merge_policy_for_consumed_slot`,
  `bindings_for_consumed_slot`, `slot_lookup` and `Vec<SlotPathSegment>::clone`
  are gone from the top twenty.

  The **4-fixture workload moved only −1.9%** (65,811,630 → 64,531,359),
  because `HwRegistry::endpoint_status_for` is 46.7% of that profile and is
  untouched by this work. The endpoint-status half is what now caps
  multi-fixture fps.

  Two smaller per-frame re-derivations surfaced while measuring, both the same
  shape and neither addressed: the shader node re-reads its consumed-slot
  *definitions* every frame through `format!`-built paths (now the largest
  single allocation source in the 1-fixture profile), and a resolver cache hit
  still clones a `ProductionSource` carrying a `SlotPath`.

  **Desk-S3 re-measured 2026-08-01** (same board `d8:3b:da:47:29:70`, same
  projects):

  | Config | Before | After | |
  |---|---|---|---|
  | 4 fixtures | 20 fps / 48 ms | **25 fps / 37.5 ms** | +25% fps |
  | 1 fixture | 50 fps / 19 ms | **67 fps / 13.5 ms** | +34% fps |

  Hardware beat the emulator's prediction for 4 fixtures (+25% vs +2%): the
  profile uses the **virtual** WS281x driver, whose endpoint enumeration is
  dearer than the real RMT driver's, so `endpoint_status_for`'s 46.7% share is
  inflated there. The emulator attributes cost well; it does not predict fps.

  Flash cost on the C6 (the tight budget): **+10,208 B (+0.36%)**, headroom
  282,432 → 272,224 B, still 4× the 64 KB CI gate. The debug-only invalidation
  guard is absent from release firmware (verified on the linked ELF).

  **The filed shape is only partly fixed.** Per-additional-chain cost went
  ~9.7 ms → ~8.0 ms (−17%) — most of the win is fixed per-frame overhead, not
  the per-chain scaling this entry is named for. A 10-fixture project would
  still be in the low teens. Endpoint status owns that scaling.

**Exit criteria** — A profiled optimization pass that makes resolved bindings
and endpoint status persist across frames (invalidate on tree/binding/
hardware change, not per tick), after which frame cost is dominated by actual
rendering work and the profile's top self-cycle entries are no longer
allocator/memcpy/string machinery. Re-measure the same quad-strips matrix on
the desk S3 as the oracle.

- [x] **Dataflow resolver** — done 2026-07-31, see the incident log above.
- [ ] **`HwRegistry::endpoint_status_for`** — per-frame re-enumeration of
      hardware endpoints with per-endpoint status recomputation and string
      spec formatting. 46.7% of the 4-fixture profile; the remaining cap on
      multi-fixture fps.
- [x] **Desk-S3 re-measurement** — done 2026-08-01 for the resolver half
      (20 → 25 fps at 4 fixtures). Re-measure again after endpoint status.
