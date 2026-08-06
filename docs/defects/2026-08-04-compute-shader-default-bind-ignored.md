---
status: fixed          # 2026-08-04 — one block covers both shader kinds
found: 2026-08-04      # how: writing timebase-uniform tests, which had to restate `time` under `bindings` on every compute node
area: lpc-engine/project_loader (slot-declared `default_bind` registration)
class: silent-drop
related:
  - docs/defects/2026-08-02-authored-source-bindings-silently-dropped.md
  - docs/adr/2026-07-09-declarative-default-bindings.md
---
# `default_bind` on a compute shader's consumed slot was ignored

**Symptom** — this compute node loads without error and its `time` slot
resolves to the slot default (0.0) forever; the shader never animates:

```json
{
  "kind": "ComputeShader",
  "source": { "path": "compute.glsl" },
  "consumed": {
    "time": { "kind": "value", "value": "f32", "default": 0.0,
              "default_bind": "bus:time" }
  }
}
```

The same slot on a `Shader` node wires to `bus:time` at fallback priority.
Nothing distinguishes the two in the artifact, in the load result, or on
the wire — the compute node simply has no binding.

**Cause** — the kind-owned plumbing match in `register_node_bindings`
(lp-core/lpc-engine/src/engine/project_loader.rs) had a `NodeDef::Shader`
arm that iterated `consumed_slots` for `default_bind` and registered each
one. `NodeDef::ComputeShader` fell through to `_ => {}`. The generic
`register_declared_defaults` pass that runs afterwards walks the *declared*
def/state shapes for a kind, and a shader's consumed slots are
artifact-declared, not shape-declared — so nothing else covered them. The
authored-`bindings` path worked, which is why the gap read as "compute
shaders just want explicit bindings" rather than as a bug.

**Blast radius** — no shipped example is mis-wired: all four compute defs
(`examples/meteor/sim.json`, `examples/fluid/compute.json`,
`examples/events/event_a.json`, `examples/events/event_b.json`) restate
`time` under `bindings`, which is exactly the workaround the defect forces.

The one thing that *was* broken is the starter.
`starter_compute_shader_def` (lp-core/lpc-model/src/nodes/starter.rs)
builds its consumed slots from `starter_time_consumed_slots()` — the
shared helper whose whole purpose is "default-bound to the project clock
bus so the scaffold animates without manual wiring." Every compute node
created from the starter in the Studio came up with an unwired `time`
while its render-shader sibling, built from the same helper, came up
wired.

**Fixed 2026-08-04** — the arm became one block keyed on either shader
kind, so `default_bind` on an artifact-declared consumed slot registers
the same way for both. Pinned by
`char_compute_shader_slot_default_bind_registers` and
`char_authored_compute_time_suppresses_the_default` alongside the existing
render-shader pair.

**Still not supported: `default_bind` on a *produced* slot.** Publishing a
produced compute slot takes an authored `target` entry; a `default_bind`
there registers nothing. That is the same silent shape as this defect and
is left open deliberately — no author has asked for it, and the loader
would have to decide the channel-contention story for an
automatically-published compute output first. If it is wanted, the
direction is a `SlotDirection::Produced` pass over `produced_slots` in the
same block, whose suppression rule (`binding_target` present → skip) is
already implemented in `register_default_bind`.
