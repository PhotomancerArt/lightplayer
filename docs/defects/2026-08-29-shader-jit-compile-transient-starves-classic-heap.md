---
status: open                # silicon re-measure pending; host + emulator attribution done 2026-09-02
found: 2026-08-29
area: shader GLSL→JIT compile transient vs the classic's ~186 KB arena
class: arena-retained-transient
related:
  - 2026-08-29-load-project-resets-instead-of-refusing.md
  - 2026-09-01-hir-place-clones-exhaust-c6-heap-at-compute-compile.md
  - ../adr/2026-08-28-project-reads-bounded-streamed-refusable.md
  - ../../lp-shader/lpvm-native/tests/xt_compile_peak_memory.rs
  - ../../lp-core/lpc-engine/tests/example_shader_compile_peak_memory.rs
---
# Shader JIT compile transient eats >100 KB and OOM-resets zook on the classic

**Shape** — classic bring-up bench (2026-08-29, dig2go): loading
`/projects/zook-dome` (1.5 KB GLSL shader, 32×32 canvas, 1,500 lamps)
on an idle classic. The load itself is modest and ACKS SUCCESSFULLY —
~40 KB (165 K free → 126 K free, `[mem]` markers) — then the first-tick
shader compile drives the arena from ~126 KB free to total exhaustion:

    OOM: alloc 768 bytes failed (align 4) in shader node: compile
    [OOM] free list: holes=7 largest=608 total=1464

Two consecutive boot loops (auto-load → compile → OOM → reset), then
the recovery ledger red-gates `shader-compile:glsl` (crashCount 3) and
the board stabilizes: zook ticks at 22.85 fps with black output,
22.6 KB free / 20.7 KB largest block. This is the walk's "zook is
heap-starved" finding, now attributed: **the compiler's transient
allocations, not fixture maps or output buffers, are what eat the
heap** — a >100 KB working set to compile 1.5 KB of GLSL into a
24 KiB JIT region (which itself stayed at used=0, cap=24576 — the
compile never got far enough to emit).

**Why it matters** — zook-scale (~1.5 K LEDs) is squarely the classic's
target envelope; the sibling walk finding proved the wire refusal gate
correctly declines its reads at 19 KB largest block, but the shader
never renders. Either the compile transient shrinks to fit, or the
classic cannot run authored GLSL at zook scale and the envelope doc
should say so.

**Next** —
- Measure the compile's peak allocation profile on host (rt_emu is the
  oracle) for zook's `shader.glsl`: which pass holds the peak (parse,
  IR, regalloc, emit)?
- Check whether the compile runs while the project's own load-time
  allocations are still resident vs. deferred-to-first-tick (it OOMs
  post-load, so the project working set is already resident — a
  compile-before-attach ordering might clear headroom).
- The 768 B terminal ask with holes=7/largest=608 says exhaustion, not
  fragmentation: the transient needs a budget/streaming fix, not an
  allocator fix.
- Recovery behavior is correct (red gate, board stays up) but the
  client-facing story is "black output, no error" — the node-status
  pipeline should surface the red gate as a node error the way
  placeholder kinds are surfaced.

**Repro** — `loadProject /projects/zook-dome` on a classic with
`examples/zook-dome` installed; watch `[mem]`/`[JIT]`/OOM lines at
921600 baud. 3/3 on this bench (two resets, then red-gated).

**Measured (2026-09-02, host tracking-allocator probes + RV32 emulator;
PR #497)** — the "Next" list above, answered:

- *Which pass holds the peak?* The frontend's HIR build, for every
  shader in the corpus but two tiny ones. Zook's whole GLSL → Xtensa
  compile peaked at **46,369 B host** on the `#495` tree (`lpvm-native`'s
  `xt_compile_peak_memory` sentinel), of which 23,444 B was the transient
  of typing `render_2d` on top of a resident token tape (9.5 KB), parsed
  bodies (8.7 KB) and the HIR itself at 184 B per expression.
- *Device-width:* the RV32 emulator (`lp-cli profile --collect alloc
  --mode startup examples/zook-dome`) measured the `shader-compile` window
  at **23,757 B transient, 4,990 B retained** — host/device ratio 1.95×,
  the same ratio as basic (1.94×) and meteor (1.94×). The ">100 KB"
  figure in the shape above is not what the compiler costs today: either
  it predates the sample-out/read-back fixes (#474/#475) or it folded the
  JIT link and the first frame's residents into the compile bracket. The
  premise "the classic cannot run authored GLSL at zook scale" no longer
  holds on the numbers; what is still owed is a silicon `[mem] shader
  compile before/after` bracket on a classic, which is why this stays
  `open`.
- *What shrank it further:* the per-node-copies plan
  (`2026-09-02-0817-hir-per-node-copies-corpus`) took zook to **30,788 B
  host** (the peak is now the backend's `lower` pass for `render_2d`,
  no longer the frontend) and the xt sentinel to 26,971 B: token tape
  dropped after the header step, 16-byte place segments in one arena
  list, 56-byte HIR expressions (import ids, arena writeback lists,
  compact swizzles, boxed texture operands, a per-function type table).
  On the emulator the window only moved 23,757 → **22,858 B**: zook's
  peak is now the backend's `lower` pass, whose structures carry few
  pointers, so the frontend's ~1.9× host/device ratio no longer applies
  and ~23 KB is the backend's own transient — against ~126 KB free after
  load. Further zook savings are a backend question.
- *Streaming/budget fix:* not needed at this size; the ceiling tests pin
  the shape instead (xt sentinel 37 KB, px corpus 112 KB host).
- *The residents the compile runs on top of* are now tabulated per owner
  and per lamp (`docs/reports/2026-09-02-per-lamp-memory-table.md`, plan
  `2026-09-02-2154-per-lamp-memory-table`): zook's first-frame residents
  went from 49 to 29 B/lamp device-side (the sample-out scratch copy and
  the output node's two extra copies of every sample removed) and the
  load peak from 32 to 8 B/lamp — 30 KB more headroom on the classic at
  the moment this compile runs. Silicon bracket still owed.

The other three findings stand: the compile still runs post-load with
the project resident; exhaustion, not fragmentation, was the failure
shape; and a red-gated compile still renders as black with no node
error (see `2026-09-01-2026-fault-is-never-black`).
