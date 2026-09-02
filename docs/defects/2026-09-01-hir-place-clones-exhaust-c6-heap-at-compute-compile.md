---
status: fixed
found: 2026-09-01          # how: hardware-walk (XIAO ESP32-C6 bench, Meteor pushed from Studio)
fixed: this change
area: lps-glsl HIR build (hir/typeck place typing, hir/place) + lower/place
class: arena-retained-transient
related:
  - 2026-08-29-shader-jit-compile-transient-starves-classic-heap.md
  - 2026-08-31-c6-rmt-ws281x-dark.md
  - ../../lp-core/lpc-engine/tests/meteor_compute_compile_peak_memory.rs
  - ../../lp-shader/lpvm-native/tests/xt_compile_peak_memory.rs
---
# HIR place clones exhaust the C6 heap compiling meteor's compute shader

**Symptom** — XIAO ESP32-C6, firmware `c3514826e056`, the Meteor example
pushed from Studio as `/projects/studio`. Every boot, the auto-load
succeeds with room to spare and the very next tick's compute-shader
compile takes the whole 300 KB heap:

    [mem] boot auto_load after: 194848 B free / 105152 B used (190k / 102k)
    allocation failed: requested=252 align=4 free=128 used=299872 largest_free=120
        retry_ok=false context=compute shader node: compile
    [RECOVERY] last run crashed (oom): at node:/studio.show/o/…/node:/studio.show/s

Decoding the OOM frames against the same build puts the failing
allocation inside the GLSL frontend's type checker, cloning a struct
type: `Vec<StructMember>::clone` ← `LpsType::clone` ←
`TypeCtx::type_place` ← `TypeCtx::type_expr`. After two such boots
lp-recovery quarantines the node and the project renders black at
43 fps; the device card says "Running" throughout (the recovery status
never reaches the heartbeat mirror — its own defect). The same OOM pairs
sit in the 2026-08-31 journal: this had been marginal for days, and the
previous build fitting by ~250 B was luck, not margin.

**Root cause** — not a marginal compile, a quadratic one. The HIR
arena kept, for every `meteors[i].field` reference in `sim.glsl` (40 of
them), about five full copies of the `Emitter` struct-array type — the
struct, every member, every member *name* — until the function lowered:

1. `type_place` was recursive *through the arena*: typing `a[i].f`
   pushed `a`, then `a[i]`, then `a[i].f` as three separate places, each
   built by cloning the previous one (root, segments, type and all).
   Nothing ever read the two intermediates, but the arena is not freed
   until the function lowers, so they stayed.
2. `PlaceRoot::{Uniform,Global,Param,Local}` each carried a full `ty`
   *in addition to* `HirPlace.ty` — two copies of the array-of-struct
   type per root place, before any projection narrowed anything.
3. `PlaceSegment::Index { ty }` stored the element type (the whole
   struct), although lowering already re-derives it from the type it
   indexes (`apply_index` → `TypeShape::array_element`).

The host probe (`meteor_compute_compile_peak_memory`) measures the
compile's transient peak at **317,600 B host** before the fix, all of it
in `frontend:build-hir` for `tick()`, with 288,883 B still resident
when the HIR build finished. Zook's whole px-shader compile (the
classic's defect, #471) peaks at 46 KB by the same method. Host bytes
overstate device DRAM ~1.5–2×; a ~195 KB device peak against 195 KB free
is exactly the observed cliff.

`sim.glsl` is not unusual — it is what a persisted struct-array global
looks like when the sim writes every field every tick. The render
shader, with 17 references to the same `Meteor` uniform array, compiled
in 74 ms on the same boot; it was simply under the cliff.

**Fix** —
- `TypeCtx::build_place` types a place expression by value through the
  recursion and `type_place` pushes only the finished place (1).
- `PlaceRoot` variants carry no type; `HirPlace::{local,param,uniform,
  global}` set only `HirPlace.ty`. Lowering's `root_place` (and the
  global-read arm of `place_read`) look the root type up from the
  function's param table or the module's `uniforms`/`globals`, which
  `LowerCtx` now carries (2).
- `PlaceSegment::Index` carries only the index expression; the element
  type comes from the new `hir::index_element_type` — the same function
  `push_index` narrows with — applied to the value being indexed (3).
- Engine: the shader nodes bracket their compile with `[mem] shader
  compile before/after` lines through a process-wide stats hook
  (`lpc_shared::memory`, installed by `LpServer`) so the next bench
  journal shows the device-side number instead of only its absence.
  (`lpc-shared/src/memory.rs` existed but was never a module of the
  crate; it is now.)

Measured (host bytes, transient peak above the pre-compile baseline,
`frontend:build-hir` in every case):

| step | peak | build-hir resident |
|---|---|---|
| before | 317,600 | 288,883 |
| places by value (1) | 164,776 | 136,059 |
| + no root/index types (2, 3) | **116,392** | 87,675 |

The remaining resident set is the HIR itself (one arena entry per
expression, ~40 KB host for `tick()`) plus the parsed bodies the build
still holds; both scale with source size, not with references × type
size. Device validation (the Studio flash/push flow on the bench, the
`[mem]` bracket in the journal) is the next bench session's job — the
host number is the proof of shape, not the margin.

**Regression coverage** —
`lp-core/lpc-engine/tests/meteor_compute_compile_peak_memory.rs`:
`meteor_sim_compile_peak_profile` pins the meteor sim compile's transient
peak under 160 KB host (≈1.4× the fixed measurement, half the unfixed
one); `every_example_compute_shader_compile_peak` runs every checked-in
example's compute shaders (events, fluid, meteor) through the same
pipeline under the same per-shader ceiling and prints the comparison.
Both compose the compiler input through the node's own
`compute_glsl_source` seam, header included, so they compile what the
device compiles. The lps-glsl/lp-shader suites (151 tests) cover the
lowering rewrites; the filetest corpora cover the emitted IR.

**Lesson** — an arena is a lifetime decision, not a container: anything
pushed into it lives until the pass ends, so a recursive builder that
pushes its *intermediates* turns a transient into a resident, and a
type stored "for convenience" on every node is a clone per reference
of something that has exactly one home. The tell was in the numbers
before it was in the code — a 2.7 KB shader with a ~190 KB compile
transient cannot be paying for its own size — and the device's
`[mem]` bracket around the load was what made that arithmetic possible
in the first place. The same bracket now exists around the compile.
The other three directions the investigation weighed remain true and
are worth recording: the C6 heap cannot grow from main RAM (the main
task stack is the 32 KB left above `.bss`, and the compiler's call
depth needs it), but the 64 KB `dram2_seg` the ESP-IDF bootloader
leaves behind (`esp_alloc::heap_allocator!(#[ram(reclaimed)] …)`) is an
untouched second region if margin is ever wanted on top of a fix;
project residents (~45 KB load, ~60 KB boot baseline with the radio) were
not the problem here; and no example is "unusually heavy" — every
compute shader in the examples now sits under the same ceiling.
