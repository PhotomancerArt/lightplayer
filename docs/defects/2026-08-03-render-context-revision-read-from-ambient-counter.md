---
status: fixed
found: 2026-08-03      # how: chasing a ~1-in-13 flake in `studio_edit_e2e` on clean main
fixed: 2026-08-03
area: lpc-engine/engine (EngineResolveHost render contexts)
class: two-clocks
related:
  - docs/adr/2026-08-03-memory-pressure-at-compile-safe-points.md
  - docs/adr/2026-07-03-revision-gated-project-reads.md
---
# A render context read its revision from the ambient counter, so the compile window it was granted never matched

**Symptom** — `app::studio::studio_edit_e2e_tests::successive_shader_applies_each_reach_the_engine`
failed roughly 1 run in 13 of `cargo test -p lpa-studio-core studio_edit_e2e`,
at either the first or the second assertion, always the same way: the
`.expect("… surfaces a compile error")` found `None`. A shader body with a
deliberately unknown identifier was applied, a frame was ticked, the project
was refreshed — and the node reported no error at all, as if the bad source
had never reached the engine.

The test never failed in isolation (0/60) and never failed with
`--test-threads=1` (0/25). Only the parallel run failed.

**Cause** — two revisions that are supposed to be the same number came from
two different clocks.

`Engine::tick_nodes` stamps the frame: `self.revision = advance_revision()`.
When any node is waiting on a compile window it then stamps that same value
onto every node — `node.open_compile_window(self.revision)`. A shader only
compiles when its render context reports the revision its window was opened
for:

```rust
if self.compile_window != Some(ctx.revision()) && !self.compile_window_requested {
    self.compile_window_requested = true;
    return Ok(self.shader.is_some());   // defer
}
```

But `EngineResolveHost::render_node_texture` (and its four siblings) built
that render context from `lpc_model::current_revision()` — the **ambient
process-global counter** — rather than from the engine's frame revision.
The two agree only as long as nothing else advances the ambient counter
between the tick's `advance_revision()` and the node's render.

In the `lpa-studio-core` test binary something does. `lpc-model` is compiled
there without its `test-support` feature, so `CURRENT_REVISION` is a
process-global `AtomicI32` rather than a thread-local — and 22 tests, each
driving its own engine, share it. Instrumented, the failing run reads:

```
tick revision=Revision(172) window_wanted=true
ensure_compiled node=NodeId(4) ctx_rev=Revision(173) window=Some(Revision(172))
DEFER compile node=NodeId(4)
```

A sibling test's tick landed in between and moved the ambient counter by one.
The node deferred the compile it had just been granted a window for, the
studio's `RefreshProject` read the node's status one frame too early, and the
compile error the test was waiting for did not exist yet.

**Why it matters beyond the test** — the ambient counter is process-global in
production too. Any second engine in the same process (a preview host slot,
a device session, a second project) ticking between another engine's frame
stamp and its renders desyncs the pair exactly the same way. The failure is
silent and self-healing — the deferral latch means the compile lands one
frame later via the progress-guarantee path — so in the app it reads as a
shader that occasionally takes an extra frame to show its compile error, not
as a bug with a name.

**Fix** — `EngineResolveHost` now carries the engine's `frame_revision`, set
from `self.revision` / `eng.revision` at all eight construction sites, and
the five render-context builders use it instead of `current_revision()`. The
window stamp and the render's `ctx.revision()` are now the same number by
construction, and `lpc-engine` no longer imports `current_revision` at all.

**Regression** — `compile_window_survives_an_ambient_revision_bump_inside_the_tick`
(`lp-core/lpc-engine/src/engine/project_loader.rs`) attaches a probe node that
advances the ambient revision from `handle_memory_pressure`. Pressure is
broadcast at the top of the tick, after the frame revision is stamped and
before any render, so that bump reproduces the desync deterministically. The
test asserts the boot compile still lands inside the window frame. It fails
on the old code and passes on the new.

**The class** — a value that two sites must agree on, sourced from a shared
mutable global at one site and from local state at the other. The equality
holds by coincidence (one writer) rather than by construction, and the second
writer arrives much later, from somewhere unrelated.
