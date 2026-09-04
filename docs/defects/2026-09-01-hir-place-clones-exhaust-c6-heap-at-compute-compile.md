---
status: fixed
found: 2026-09-01          # how: hardware-walk (XIAO ESP32-C6 bench, Meteor pushed from Studio)
fixed: this change
area: lps-glsl HIR build (hir/typeck place typing, hir/place) + lower/place
class: arena-retained-transient
related:
  - ../adr/2026-09-02-esp32c6-ram-split.md
  - 2026-08-29-shader-jit-compile-transient-starves-classic-heap.md
  - 2026-08-31-c6-rmt-ws281x-dark.md
  - ../../lp-core/lpc-engine/tests/example_shader_compile_peak_memory.rs
  - ../../lp-shader/lps-filetests/tests/compile_peak_memory_corpus.rs
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
size.

**Bench validation (2026-09-02, XIAO ESP32-C6, Studio card flash + push
of Meteor, capture-sink journal)** — the compile now succeeds on the
board on both paths, bracketed by the new `[mem]` lines:

    [mem] compute shader compile before: 168k free / 149k used
    [mem] compute shader compile after:  165k free / 152k used
    [compute-shader-node] compilation succeeded (elapsed=80ms, … final_code_size=1556 bytes)
    [mem] shader compile before: 157k free / 160k used
    [mem] shader compile after:  150k free / 167k used

Meteor runs at 26 fps with ~150 KB of heap free after both compiles
(against 128 B short before). The first push of the fixed image also
surfaced the *next* cliff: with the sim node ticking for the first
time, the 32 KB main stack overflowed in the resolver chain under
`ComputeShaderNode::produce` — that is ADR
`2026-09-02-esp32c6-ram-split` (72 KB stack, heap +64 KB from
`dram2_seg`, a painted-stack high-water probe: 36,936 B of 72,768 B in
steady state). The heap figures above are from that final layout
(325,536 B total).

**Regression coverage** —
`lp-core/lpc-engine/tests/example_shader_compile_peak_memory.rs::example_shader_compile_peaks`
(grown from the meteor-only probe on 2026-09-02) runs every checked-in
example's shaders — compute through the node's own `compute_glsl_source`
seam, header included; px through `px_compile_inputs` (palette texture
specs + entry space), the two synth wrappers and the backend for both
device ISAs — under the tracking allocator, prints one ranked table, and
pins each transient peak under a ceiling (compute 80 KB host, px 112 KB,
Xtensa backend 74 KB after the follow-up below). One `#[test]` on
purpose: the allocator counters are process-wide, and two tests in the
binary measured each other's allocations on CI's parallel threads.
`lps-filetests/tests/compile_peak_memory_corpus.rs` sweeps the whole
filetest corpus (802 files) through the frontend the same way. The
lps-glsl/lp-shader suites cover the lowering rewrites; the filetest
corpora cover the emitted IR.

**Recurrence (2026-09-02, the same class, every other node kind)** —
the corpus sweep asked whether more of "a copy per node of something that
has exactly one home" remained, and it did, on every node kind except the
place root this entry fixed: `HirExpr.ty`, `HirLocal.ty`, each function's
`params`/`return_ty`, and `PlaceSegment::Field { name: String, ty }`
each owned a full `LpsType` — a struct clone, member names and all, when
the value was struct-typed — and `HirExpr` was 184 B host because a call
node carried its import key's `String`s inline. The filetest
`struct/deep-nested.glsl` (5.3 KB of three-level nested structs) cloned
its `Point` member vector 4,380 times and held 320 KB of HIR; nothing
that shape is in the examples yet, but the language allows it and the C6
does not. The follow-up plan (`2026-09-02-0817-hir-per-node-copies-corpus`,
PR #497) dropped the token tape after the header step, shrank a place
segment to 16 bytes in one arena list, took `HirExpr` to 56 B, and gave
each function one type table that expressions, places, locals and the
signature index into. Meteor's sim compile: 116,392 → 58,147 B host
(60 → ~30 KB device); basic 150,317 → 81,362; deep-nested 382,991 →
183,997. What deep-nested still holds is one copy of the module's
structs *per function's* table — module-wide interning is the open
follow-up. The lesson generalises the one above: **a node stores an
id; the thing with one home stores the value.**

**Lesson** — an arena is a lifetime decision, not a container: anything
pushed into it lives until the pass ends, so a recursive builder that
pushes its *intermediates* turns a transient into a resident, and a
type stored "for convenience" on every node is a clone per reference
of something that has exactly one home. The tell was in the numbers
before it was in the code — a 2.7 KB shader with a ~190 KB compile
transient cannot be paying for its own size — and the device's
`[mem]` bracket around the load was what made that arithmetic possible
in the first place. The same bracket now exists around the compile.
The other three directions the investigation weighed: the C6 heap
cannot grow from main RAM (the main task stack is what is left above
`.bss`, and the bench then showed even 32 KB of it was ~4 KB too
little) — so the heap *shrank* there and grew by the 64 KB `dram2_seg`
the ESP-IDF bootloader leaves behind (ADR `2026-09-02-esp32c6-ram-split`);
project residents (~45 KB load, ~60 KB boot baseline with the radio)
were not the problem; and no example is "unusually heavy" — every
compute shader in the examples now sits under the same ceiling.
