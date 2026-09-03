---
status: open
found: 2026-09-02      # how: live-debugging (P3 of the fault-is-never-black plan ran the ignored emu suite)
area: lpc-engine compile-window deferral vs out-of-tick render probes; fw-tests recovery_emu (ignored suite)
class: stand-in-divergence
related:
  - 2026-09-01-silent-black-under-node-quarantine.md
  - ../adr/2026-08-03-memory-pressure-at-compile-safe-points.md
  - ../adr/2026-09-02-fault-is-never-black.md
---
# The first render probe of a never-ticked shader answers black, and the ignored emu test that would catch it fails on main

**Symptom** — `fw-tests/tests/recovery_emu.rs`
`fuel_exhausted_shader_errors_without_reboot_or_blame` (the whole file is
`#[ignore]`; only `just test-recovery-emu` runs it, and no CI job does) fails
on `origin/main` (22e80185d) exactly as on the fault-is-never-black branch:

```
expected a probe error, got Texture { product: VisualProduct { node: NodeId(5), output: 0 },
revision: Revision(6), width: 4, height: 4, format: Rgba16, bytes: [0, 0, 0, …] }
```

The test's fuel-hungry shader sits on an unconsumed bus channel, so nothing
ticks it; its FIRST `RenderProduct` probe expects the fuel diagnostic and
gets an all-zero 4×4 texture instead. The second probe traps as expected.

**Root cause (read, not yet pinned by a test)** — the compile-window
deferral (`ShaderNode::ensure_compiled`, ADR 2026-08-03): the first render
that wants a compile only REQUESTS a window and renders black (`Ok`), and
the engine opens windows for every alive node at the top of the next tick
stamped with THAT tick's revision. A shader only ever reached by an
out-of-tick probe sees `compile_window != ctx.revision()` on its first
probe, requests, and answers black; the compile proceeds on the second
render by the progress guarantee. Either the test predates the deferral or
the probe path once opened its own window; the suite being ignored and
outside CI is how the divergence survived.

**Why it matters** — a probe is the studio's way of asking "what does this
node render"; a black answer on the first ask is the same shape as the
silent-black class, one level up. It is also a test-rot signal: an ignored
suite that fails on main protects nothing.

**Fix direction** — either (a) the probe path opens a compile window for
the probed node before rendering (a probe is a safe point: no render
borrow is live), or (b) `read_project_render_product_probe` renders twice
when the first render was a deferral, or (c) the test ticks once between
load and the first probe and the deferral is documented as probe
behaviour. (a) is the honest one. Whichever lands, un-ignore or CI-gate the
suite so it cannot rot again.

**Regression coverage** — none yet: the existing ignored test IS the
coverage once the fix lands.

**Lesson** — an `#[ignore]`d hardware-shaped suite needs a scheduled run or
a CI job, or its failures are indistinguishable from "nobody ran it".
