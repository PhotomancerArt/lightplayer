---
status: open
found: 2026-08-29
area: shader GLSL→JIT compile transient vs the classic's ~186 KB arena
related:
  - 2026-08-29-load-project-resets-instead-of-refusing.md
  - ../adr/2026-08-28-project-reads-bounded-streamed-refusable.md
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
