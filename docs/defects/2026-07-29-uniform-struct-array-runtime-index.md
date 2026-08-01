---
status: fixed
found: 2026-07-29      # how: live-debugging (Yona opened the meteor sim)
area: examples/effects/meteor (render.glsl), lps-frontend lowering, example shader coverage
class: missing-coverage
related:
  - docs/debt/example-shaders-not-compile-gated.md
---
# Meteor's render shader indexed a uniform struct array with a runtime value

**Symptom** — opening the imported meteor effect in the browser sim:

```
shader compile: lower: in function 'drawMeteor': unsupported expression:
AccessIndex: struct value behind Load: Access base has no uniform element addr
path /studio.show/meteor.project/render.shader
```

The effect mounted and ran; only its visual failed to compile.

**Root cause** — `drawMeteor(vec3 accum, int slot, vec2 uv)` read
`meteors[slot].…` where `slot` is a runtime parameter. A uniform array of
structs has no element address resolvable at lower time, so members must be
read through **constant** indices at the call site and passed in as
scalars/vectors. `examples/events/shader.glsl` already carries exactly that
shape (`drawEvent(color, 0, events[0].id, events[0].seq, uv)`) — the
existing example was the idiom, and the new one departed from it.

Measured breadth (filetest probe, 2026-07-29): the runtime-indexed form
**compile-fails on 4 of 5 targets** — `rv32n.q32`, `rv32c.q32`, `wasm.q32`,
`interp.f32` — and compiles only on `rv32lpn.q32`. It is broadly
unsupported, not a single-target quirk.

**Fix** — `examples/effects/meteor/meteor/render.glsl` now takes the
members as parameters (`uint id, vec2 head, vec3 color, float intensity`)
and indexes with constants at the call site. (The meteor example itself
lands with the composite-effects work, PR #218; the filetest and this
entry were harvested ahead of it.)

**Regression coverage** — the miss is the interesting part, and it was two
independent holes:

1. **The engine render test could not see it.** The new
   `effect_examples_render_through_their_mirrors` (lpc-engine) renders each
   effect and asserts nonzero RGB. It runs the HOST backend, whose lowering
   accepts the construct — verified empirically by restoring the broken
   shader, which still passed. Adding a per-node
   `NodeRuntimeStatus::Error` assertion did not catch it either: on the
   host path there is genuinely no error to report. An engine-level render
   test is structurally incapable of gating shader-lowering support.

2. **Example shaders are not compile-gated on the other targets.** Nothing
   in CI compiles `examples/**/*.glsl` for the device/browser-canonical
   targets, so any example can ship a construct that fails everywhere but
   the host. That condition is broader than this defect — see the debt
   entry.

Added `lp-shader/lps-filetests/filetests/uniform/array-of-struct.glsl`,
which pins the SUPPORTED idiom (constant-index member reads, including
through a helper) green on all five targets. Note a negative test would not
gate anything: the filetest harness records `compile-fail` as an
*expected-failure*, not a failure.
