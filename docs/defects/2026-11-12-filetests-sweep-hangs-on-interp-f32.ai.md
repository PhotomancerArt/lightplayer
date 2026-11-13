---
status: open
found: 2026-11-12      # how: planning the one-GLSL-frontend switch (23 CPU-minutes, no output), and again during it (killed after 6 minutes)
area: lps-filetests-app sweep × interp.f32
class: untested-path
---
# The filetests sweep never finishes on interp.f32

**Symptom** — `lps-filetests-app sweep -t interp.f32` (every `// run:`
directive, annotations ignored, one JSONL record each) ran for 23
CPU-minutes without writing its output, and a second attempt was killed
after six. The same corpus on `wasm.q32`, `wasm.f32` and `rv32c.q32`
sweeps in seconds to a few minutes.

**Why** — the sweep runs every directive, including the ones annotated
`@unsupported(interp.f32)` because they would not terminate (an unbounded
loop the interpreter has no fuel for: `fuel/` and `control/torture/` files
say so). The test runner skips those; the sweep has no fuel limit or
per-directive timeout of its own, and its output stayed empty while it ran,
so one hung directive loses the whole run.

**Cost** — the sweep is how stale per-target annotations are found. During
the one-frontend switch, interp.f32's stale `@unsupported` lines had to be
checked another way (strip, run the test runner, restore).

**Fix direction** — give the sweep's interp path an op budget
(`lpir::InterpLimits::with_fuel`) and report an exhausted directive as its own
status, and flush each record as it completes.
