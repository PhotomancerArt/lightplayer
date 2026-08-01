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

> **2026-08-01 status:** both mechanisms named below are fixed (PRs #243,
> #244). The per-chain cost they were blamed for moved only 9.7 → 8.0 ms, so
> the linear degradation this entry exists for **remains open** and is now
> unattributed. See the 2026-08-01 incident-log entry.

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

**Carrying cost** — Multi-fixture projects degrade linearly. As filed:
~8.4 ms per fixture+output chain, 4 fixtures = 20 fps, ~10 fixtures ≈
single-digit fps. **After both fixes (2026-08-01): ~8.0 ms per chain, 4
fixtures = 25 fps, ~10 fixtures ≈ 12 fps.** The linear term is essentially
unchanged; what improved was fixed per-frame overhead. Authors still cannot
buy the cost down with lower resolution or fewer LEDs, which is what makes
the scaling feel arbitrary from the outside.

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
- **2026-07-31 (later)** — **Endpoint-status half closed** (PR #244,
  plan `2026-07-31-2224-hw-endpoint-status-cache`, ADR
  `2026-07-31-output-sink-retry-policy.md`). The 45.8% was not status lookup
  but a **failed-open retry storm**: `ensure_channel_open` re-attempted any
  handle-less sink every frame, and the emulator board declared one WS281x
  channel and no `D9`/`D8`/`D7`, so three of quad-strips' four sinks could
  never open and re-enumerated the whole board — 256 endpoints, each with a
  formatted spec and a live status — sixty times a second, forever.

  Fixed by *not asking*, not by caching: sinks park on a new
  `HwRegistry::generation()` (bumped only on successful claim/release) and
  wake when hardware ownership actually moves. No endpoint status is stored
  anywhere, so reserved-pin and claim-conflict semantics cannot go stale. Also
  collapsed the 3 enumerations per open attempt to 1, and stopped
  `refresh_output_sink_configs` cloning every output def per tick.

  Measured, frame-for-frame (8 frames both runs, `events.jsonl` B→E):
  **steady frame 16.42M → 1.53M cycles, 10.7×**; total attributed 65.8M →
  6.2M. `endpoint_status_for`, `VirtualWs281xDriver::endpoints`,
  `endpoint_for_spec`, `validate_spec` and the `core::fmt` machinery are all
  **absent from the top-20 self cycles**. Per-frame warn spam → 7 lines for
  the whole run. Profiles:
  `2026-07-31T22-42-28--…quad-strips--steady-render` (before) and
  `…23-29-27` (after); `…23-33-40` is after the emulator board was given the
  S3's four channels (steady frame 1.554M — the +1.3% is three more strips
  actually being written).

  **Desk-S3 re-measured 2026-08-01** (d8:3b:da:47:29:70, identified by MAC via
  `espflash board-info`; branch firmware flashed, quad-strips pushed):
  **20 fps, tick 48 ms — flat**, stable over 13 consecutive `[perf]` readings.
  That is exactly the prediction: the S3 opens all four channels on frame one,
  so it never paid this cost in steady state, and its flat ~8.4 ms/fixture is
  the resolver.

  Since an unchanged fps cannot itself prove the new image was running, the
  parked-sink path was exercised on silicon instead: quad-strips with one
  output re-pointed at `ws281x:rmt:NOT-A-PIN` produced **2 warnings and then
  silence across ~1,250 frames** (the two being the designed settle — first
  attempt, then one retry after the other three opens bumped the generation).
  The old code logs one per frame, so this both proves the image and confirms
  the fix on hardware. fps held at 20 with the dead output; tick 47 ms, the
  1 ms being one fewer strip to write.

  Not measured: the same misconfigured-output case on *pre-fix* firmware, which
  would quantify what silicon saves there. The saving is a board enumeration
  plus a serial line per frame; on the S3's ~40-resource manifest that is far
  smaller than the emulator's 256, and it was not worth a second flash cycle to
  put a number on.

  **Still open: the resolver half** — the profile is now dominated by exactly
  what this entry predicted would remain: memcpy 18.7%, allocator 12.8%+8.0%,
  `QueryKey::eq` 5.7%, `EngineSession::resolve` 2.7%, `SlotPath::parse`. That
  is the dataflow resolver re-resolving from cold each tick, owned by plan
  `2026-07-31-2225-persist-dataflow-resolution`; `…23-33-40` is its baseline.
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

- **2026-08-01** — **Both halves on one image, measured together.** Emulator
  (`quad-strips`, steady render): **65,811,630 → 3,011,467 cycles, 21.9×**, and
  `[jit] render` is now the largest engine entry (3.6%). Desk S3
  (d8:3b:da:47:29:70, both fixes flashed, project confirmed from the device
  heartbeat as `/projects/Quad strips`):

  | Project | Original | Resolver only | Both halves |
  |---|---|---|---|
  | 4 fixtures | 20 fps / 48 ms | 25 fps / 37.5 ms | **25 fps / 37 ms** |
  | 1 fixture | 50 fps / 19 ms | 67 fps / 13.5 ms | **68 fps / 13 ms** |

  The endpoint-status half contributes **nothing on silicon**, exactly as its
  own ADR predicted: the S3 opens all four channels on frame one and never
  entered the retry storm. The 21.9× is real but is an artifact of the
  emulator's virtual board declaring one WS281x channel — worth remembering
  before quoting emulator ratios as device wins.

  **⚠️ The shape this entry is named for is NOT fixed.** Decomposing the two
  measurements into fixed overhead plus per-chain cost:

  | | per fixture+output chain | fixed per-frame |
  |---|---|---|
  | Original | 9.7 ms | 9.3 ms |
  | Both halves | **8.0 ms** | 5.0 ms |

  Per-chain cost fell only **−17%**; the fixed overhead nearly halved. That is
  why 1 fixture improved 36% and 4 fixtures only 25%. Projected 10 fixtures:
  **9 fps → 12 fps** — still unusable, still linear. Authors still cannot buy
  the cost down with resolution or LED count.

  So both *named mechanisms* are fixed and the carrying cost below is not.
  Whatever owns the remaining ~8 ms per chain has not been attributed: the
  1-fixture profile's top entries are memcpy and the allocator (30% combined),
  with measured candidates being the shader node's per-frame re-read of its
  consumed-slot definitions via `format!`-built paths, per-hit
  `ProductionSource` clones, `FixtureNode::render_control`, and the 1.3 ms/
  channel blocking RMT send. **This entry stays open on that basis** — a third
  attribution pass on the 8 ms, not a third guess.

**Exit criteria** — A profiled optimization pass that makes resolved bindings
and endpoint status persist across frames (invalidate on tree/binding/
hardware change, not per tick), after which frame cost is dominated by actual
rendering work and the profile's top self-cycle entries are no longer
allocator/memcpy/string machinery. Re-measure the same quad-strips matrix on
the desk S3 as the oracle.

- [x] **Dataflow resolver** — done 2026-07-31, see the incident log above.
- [x] **`HwRegistry::endpoint_status_for`** — done 2026-07-31 (PR #244): a
      failed-open retry storm, fixed by parking sinks on
      `HwRegistry::generation()` rather than by caching status. Emulator-only
      in steady state; the S3 never paid it.
- [x] **Desk-S3 re-measurement** — done 2026-08-01, separately and jointly.
      Both halves on one image: 25 fps at 4 fixtures, 68 fps at 1.
- [ ] **The linear term itself** — the two named mechanisms are fixed and
      per-chain cost still sits at ~8.0 ms (was ~9.7). This entry stays open
      on that number, not on either mechanism. Next step is attribution of
      that 8 ms, not another guess; `projects/test/quad-strips-1fix` vs
      `quad-strips` under `lp-cli profile` is the differential that isolates
      it.
