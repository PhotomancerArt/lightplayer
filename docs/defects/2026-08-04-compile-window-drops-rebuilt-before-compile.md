---
status: fixed
found: 2026-08-04      # how: live-debugging (dome capacity discovery, M6 planning F2)
fixed: this change     # M6 P4; fill the real hash in the NEXT commit that touches the registry
area: lpc-engine/nodes (fixture + output memory-pressure handlers)
class: reclaim-ordered-behind-its-own-rebuild
related:
  - docs/adr/2026-08-03-memory-pressure-at-compile-safe-points.md
  - 2026-08-02-classic-oom-retry-succeeds.md
  - 2026-08-03-render-context-revision-read-from-ambient-counter.md
---
# The compile window dropped exactly the buffers the same tick had already rebuilt

**Symptom** — the mechanism designed to make room for a shader compile
(#303, ADR `2026-08-03-memory-pressure-at-compile-safe-points`) ran and
freed nothing. On the dome bracket the 900-LED heavy-shader compile still
OOM'd, at `used=183,024` of a 186,368 B arena — with the `High` drops
nominally active. Nothing in the ADR's own test suite disagreed, because
nothing in it asserted a byte.

(The broadcast itself was never the suspect and is not the bug: it is
unconditional and ungated at `engine.rs`, so it does reach every node on
device. There is still no logging anywhere in the pressure path, so the
device has never *printed* it — the diagnosis below is from the code
path plus the emulator allocation profile, not from a device trace.)

**Root cause** — the ADR's ordering premise is false as implemented. It
claims (§2, "Intra-tick demand order makes this airtight"):

> the fixture resolves its **visual input first** — which is where the
> shader compiles — and only then runs `ensure_direct_points` /
> sample-buffer allocation

The compile does not happen where the ADR places it. `ensure_compiled` is
reached only from `sample_visual_into` / `render_texture_into`
(`shader_node.rs`) — that is **render** time. Every buffer the `High`
handlers dropped is rebuilt **earlier in the same tick**:

| dropped at the top of the tick | rebuilt, same tick, before any compile |
| --- | --- |
| `direct_channels` | fixture `produce` → `ensure_direct_channels` |
| `sample_points`, `sample_target` | fixture render prep, *before* `sample_visual_into` |
| `control_samples` | output `produce` (`resize` to the control extent) |
| `precomputed`, `render_target` | 0 bytes on the Direct path anyway |

So **net freed at the compile instant ≈ 0 B**. The nominal droppable set
on the Direct path was 26 B/LED (4 `direct_channels` + 8 `sample_points`
+ 8 sample out + 6 `control_samples`); the realized set was nothing.

It was worse than inert. Clearing the staleness keys forced both
`ensure_direct_channels` and `ensure_fixture_sample_points` to re-run
`generate_mapping_points` **inside the window frame** — the one frame
that was supposed to be quietest. At 1500 LEDs each run allocated a
contiguous `Vec<MappingPoint>` (16 B/pt) whose doubling peak is exactly
the ask that was killing the device: 2400 × 16 = 38,400 B (30,720 B at
1200). Without the drop those `ensure_*` seams are no-ops after frame 1.
The reclaim mechanism was injecting the allocation it existed to make
room for.

**Fix** — remove the `High` drop *actions* only.
`FixtureNode::handle_memory_pressure` and
`OutputNode::handle_memory_pressure` are now no-ops carrying the ordering
rule as a seam comment. Everything else in the ADR stands and is
untouched: the unconditional broadcast at the top of the tick, the
`wants_compile_window` / `open_compile_window` protocol, the one-frame
compile deferral and its black/keep-last-good fallback, and the fluid
solver's `Critical` grid drop (which is genuinely not rebuilt by the same
tick). The real fix for the underlying capacity problem is upstream of
this seam — M6's streaming mapping-point visitor and compact resolved
carrier delete the ask itself rather than trying to free around it.

**Regression coverage** —
`memory_pressure_does_not_drop_the_fixtures_derived_caches`
(`nodes/fixture/fixture_node.rs`) and
`memory_pressure_does_not_drop_the_control_samples`
(`nodes/output/output_node.rs`) pin the no-drop behaviour by sentinel
identity at every level, and both are sabotage-verified (re-adding the
drops fails them). `memory_pressure_broadcast_leaves_the_core_path_bit_identical`
(`engine/project_loader.rs`) is the former #303 drop→rebuild differential,
kept as the identity guard any future droppable must satisfy. The
broadcast-count and window-semantics tests are unchanged.

**Why no test could catch the original defect — twice over.** First, the
#303 tests asserted *behaviour* (broadcast counts, bit-identical output)
and never a byte, so a drop that frees nothing passes every one of them.
Second, and more fundamental: on the host there are no bytes to assert.
The shader VM's wasmtime backend allocates from a bump arena whose `free`
is a documented no-op — "Bump semantics: memory is not reused"
(`lp-shader/lpvm-wasm/src/rt_wasmtime/shared_runtime.rs:112-114`) — so
only the native and RV32 backends actually return anything. A host test
that *did* try to measure reclaim would measure nothing for correct and
incorrect code alike. The falsifier was silicon, and the evidence that
finally exposed it was an allocation profile — call order and sizes — not
a pass/fail.

**Lesson** — reclaim has a *position*, not just a level. Before adding a
droppable, name two tick positions: where the transient you are making
room for runs, and where your own code rebuilds the state. If the rebuild
comes first, the drop is not reclaim — it is re-allocation, and it costs a
peak instead of buying one. The trap here is that the ordering claim was
written down, reviewed, and pinned by tests — but the tests pinned the
protocol around the claim rather than the claim, and the claim named a
seam (`ensure_direct_points`, "resolves its visual input first") one layer
away from where the compile actually is. When a design's load-bearing
sentence is about *order*, the test that protects it must observe the
order, not the outcome the order was supposed to produce.
