# ADR: Memory pressure fires at compile safe points, and compiles wait one frame for it

- **Status:** Accepted, amended 2026-08-04 (the `High` droppable set of §4
  is removed — the ordering premise in §2 was measured false; the window
  mechanism itself stands. See the Amendment section.)
- **Date:** 2026-08-03
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None

## Context

The classic ESP32's heap arena is 178,176 B. At the measured 68 B/LED a
1500-LED project is ~134 KB **at rest — it fits**. It fails only when the
GLSL compile transient (64–78 KB measured) lands on top of full per-LED
residency: 198–212 KB. Silicon measurement (2026-08-02) showed exactly this
failure shape: at 600–720 LEDs the shader compile OOMs, the recovery ladder
quarantines the node, and the device stays up rendering black. Per-LED
reduction alone can never close that gap; **not holding the transient and the
rebuildable per-LED state at the same time** can.

The seam already half-existed: `NodeRuntime::handle_memory_pressure` was
implemented on every node type with lazy `ensure_*` rebuild seams on the
fixture path — but nothing called it. Worse, the as-written fixture handler
dropped only `precomputed` (the TextureArea pixel table — **~0 bytes on the
dome's direct-sampling path**) and `direct_points`, freeing ~18 KB at
1500 LEDs where the transient needs ~40–70 KB of headroom.

Two hard constraints shape where reclaim may run:

- **Aliasing is UB, not a bug.** The render path holds `&mut [u16]` slices
  into per-LED buffers; freeing one while a slice is live is undefined
  behaviour.
- **Reentrancy deadlocks.** Freeing from inside `GlobalAlloc::alloc` runs
  `dealloc` while holding the allocator lock; `esp_alloc` and linked-list
  allocators deadlock rather than recover.

## Decision

### 1. Reclaim happens at safe points only

Pressure is broadcast at the **top of a tick**, before any node runs — no
render borrow is live there — and never from inside the allocator or
mid-render. Embedders may broadcast between ticks via
`Engine::broadcast_memory_pressure` (e.g. from the existing
allocation-failure retry hook, which runs outside the allocator lock).

### 2. Compiles defer one frame for a compile window

A shader (or compute shader) whose render wants a compile does not compile
immediately. It **requests a compile window** and renders keep-last-good (or
black, before its first compile). The engine polls
`NodeRuntime::wants_compile_window` at the top of the next tick; if any node
requested, it broadcasts `PressureLevel::High` to every alive node, then
opens a revision-stamped window (`open_compile_window`). The requesting node
compiles during that frame's render.

Intra-tick demand order makes this airtight on the render path: the fixture
resolves its **visual input first** — which is where the shader compiles —
and only then runs `ensure_direct_points` / sample-buffer allocation. So the
compile transient always runs against the lowered baseline, and the dropped
state rebuilds *after* the compile, in the same tick.

- **Windows expire with their frame.** A stale window from a tick where the
  node was not demanded must not authorize a compile long after the
  broadcast (the playlist-switch case).
- **At-most-once deferral (progress guarantee).** If a node's request is
  still standing at its next render — a host that resolves renders without
  driving `Engine::tick` never opens windows — the compile proceeds without
  one. Tick-driven hosts always broadcast before the second render, so
  pressure still precedes every compile there.
- Costs one deferred frame per compile. A compile takes up to ~100 ms and
  dropping a frame or ten during it is expected (decided 2026-08-03);
  WS281x LEDs latch their last colour, so the strip holds its frame while
  state is dropped.

### 3. The level contract

- `Low` / `Medium` — reserved, never currently broadcast.
- `High` — routine compile window. Drop only state that rebuilds **lazily
  and to bit-identical core-path output** (the `ensure_*` pattern). Dither
  and interpolation state are exempt from the identity requirement — they
  are gravy features per
  `2026-08-03-gravy-features-out-of-core-correctness-tests.md`.
- `Critical` — survival (an embedder's OOM-retry hook). May additionally
  drop **resettable** simulation state (the fluid solver grid) where the
  loss is a visible discontinuity but not a correctness failure.

Never droppable at any level: authored/synced slot data, resolved mappings,
compiled shader programs (keep-last-good), asset text.

### 4. The widened droppable set

`FixtureNode` drops `precomputed`, `direct_points`, `sample_points`,
`sample_target`, and `render_target` (all have stale→recreate seams).
`OutputNode` drops `control_samples` (fully re-established by the next
consume). The output's runtime buffer is deliberately untouched — its
lifecycle belongs to the sink-registration path, which the multi-endpoint
output work (PR #301) is restructuring. `DisplayPipeline` (~12 B/LED) is
**firmware-side state the engine broadcast cannot reach**; a firmware-side
drop at the same safe point is recorded as follow-up, not attempted here.

## Alternatives rejected

- **Allocator-level reclaim** (`Purgeable<T>`, a registry freed from inside
  `alloc`): the two hazards above. Prior art if ever revisited — Linux
  shrinkers (`register_shrinker`), `NSPurgeableData`'s explicit
  begin/endContentAccess borrow window, Java `SoftReference`, `MADV_FREE`.
  The recurring lesson: the borrow window must be explicit and dynamic.
  Revisit only if safe-point reclaim measurably underdelivers; the
  attachment point is the existing on-device allocation-failure retry hook
  (observed on silicon: `[OOM] RETRY SUCCEEDED`), outside the allocator
  lock, broadcasting `Critical`.
- **Broadcast only under low-headroom heuristics** (device-only):
  non-uniform between host and device, untestable on the emulator gate, and
  the probing tempts allocator-hook designs. Uniform engine behaviour means
  the heap-budget gate measures the same machinery the device runs.
- **Immediate compile with post-hoc reclaim**: reclaim after the transient
  frees nothing at the moment that matters — the peak.

## Consequences

- The first frame after load, and the first frame after a playlist switch
  to a never-compiled shader, render fallback (black or keep-last-good).
  Tests that render once and assert compiled output either render twice or
  open a window explicitly (`open_compile_window`).
- Pinned by tests in `project_loader.rs`:
  drop → tick → **bit-identical** published output bytes
  (sabotage-verified — a corrupted rebuild seam fails the test), exactly one
  `High` broadcast before the boot compile, a second window on playlist
  switch, and no steady-state broadcasts.
- The heap-budget ratchet record (`scripts/heap-budget-record.json`) is
  re-baselined in the same change — the deferral moves the compile into the
  second `frame` window, and the pressure drop lowers the live baseline the
  transient lands on.
- The end-to-end assertion that 1500 LEDs now fits belongs to the dome
  validation plan (blocked on multi-endpoint output, PR #301), which owns
  the dome-scale project and silicon measurement.

## Amendment (2026-08-04): the `High` droppable set is removed

The dome-scale silicon bracket and the allocation profile that followed it
falsified §2's ordering claim. **§4 (the widened droppable set) is
withdrawn. Everything else in this ADR stands.**

### What was measured

§2 says the intra-tick demand order makes the window airtight: "the
fixture resolves its **visual input first** — which is where the shader
compiles — and only then runs `ensure_direct_points` / sample-buffer
allocation." That is not where the compile is. `ensure_compiled` is
reached only from `sample_visual_into` / `render_texture_into` — **render**
time — while every buffer §4 listed is rebuilt *earlier in the same tick*
by the dropping node's own code: `direct_channels` in fixture `produce`,
`sample_points`/`sample_target` in render prep before `sample_visual_into`,
`control_samples` in output `produce`. Net freed at the compile instant is
**≈ 0 B**, against a nominal 26 B/LED on the Direct path (`precomputed`
and `render_target` are 0 B there to begin with).

The drop also made the peak *worse*: clearing the staleness keys forced
`generate_mapping_points` to re-run twice inside the window frame, each
run allocating the contiguous `Vec<MappingPoint>` whose doubling peak
(2400 × 16 = 38,400 B at 1500 LEDs) is the exact ask that was killing the
device. The broadcast itself was never in doubt — it is unconditional and
does fire on device; it simply arrived at a moment when there was nothing
left to free.

Corroborating measurement: the 900-LED heavy-shader compile OOM'd at
`used=183,024` of the 186,368 B arena with the drops nominally active.

### What changed

`FixtureNode::handle_memory_pressure` and
`OutputNode::handle_memory_pressure` are now no-ops carrying the ordering
rule as a seam comment.

### What remains in force

- **§1** — reclaim at safe points only; the top-of-tick broadcast and
  `Engine::broadcast_memory_pressure` are unchanged and still reach every
  alive node.
- **§2** — the `wants_compile_window` / `open_compile_window` protocol,
  the one-frame compile deferral, window expiry with the frame, and the
  at-most-once progress guarantee. Only the ordering *rationale* sentence
  is retracted; the deferral earns its keep on its own terms (it moves the
  transient off the frame that also allocates, and it is what the
  revision-stamped window tests pin).
- **§3** — the level contract, with one clause added by measurement: at
  `High`, a droppable must also be state **the same tick does not rebuild
  before the transient runs**. Position, not just rebuildability.
- **`Critical`** — the fluid solver's grid drop is untouched. It is the
  one real droppable: nothing rebuilds it inside the tick.
- **Never-droppable** list — unchanged.

### Follow-ups

- **`DisplayPipeline` stays open** (Q3, deferred at M6 planning): up to
  21 B/LED firmware-side (6 always, +12 interpolation, +3 dither) — ~9 KB
  at 1500 LEDs with interpolation and dither off. It is not a
  `NodeRuntime`, so the engine broadcast cannot reach it by construction;
  a firmware-side drop at the same safe point remains the open item this
  ADR recorded on 2026-08-03.
- The capacity problem this ADR was written for is being closed upstream
  instead, by deleting the allocation rather than freeing around it:
  M6's streaming mapping-point visitor and compact resolved carrier.

### What happened to the tests Consequences pinned

The drop → tick → bit-identical differential survives as
`memory_pressure_broadcast_leaves_the_core_path_bit_identical` — same
assertion, now pinning that a broadcast is *inert* on the core path, and
standing as the identity guard for any droppable added back later.
Broadcast counts, the second window on playlist switch, and the
no-steady-state-broadcast assertion are unchanged. Per-node no-drop
coverage was added beside each handler, sabotage-verified.

Full evidence chain, and why no host test could have caught this (the
wasm backend's `free` is a bump no-op, so reclaim is unobservable on the
host):
`docs/defects/2026-08-04-compile-window-drops-rebuilt-before-compile.md`.
