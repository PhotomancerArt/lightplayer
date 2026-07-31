---
status: fixed
found: 2026-07-31      # how: hardware-walk (P4 quad-strip bring-up)
fixed: this change
area: lpc-engine project loader (+ engine_services flush, virtual ws281x driver)
class: silent-drop
related:
  - docs/defects/2026-07-31-opt-z-missed-rmt-drain-deadline.md
---
# A node whose definition fails to parse is silently dropped from the project

**Symptom** — A 4-channel test project drove only 3 LED channels on the desk
ESP32-S3, with **zero** error output: `load_project` returned `Ok`, boot
logged `auto-loaded project`, and the missing channel simply never opened.
On host the same project opened a *different*, nondeterministic subset of
channels per run.

**Root cause** — Three stacked mechanisms, one primary:

1. **The silent drop (primary).** The authored project had one output node
   with an unparseable endpoint (`"ws281x:rmt:"` — a zsh 1-indexed-array slip
   in the file generator) and one fixture with invalid map2d JSON. The loader
   maps a def that failed to load to `kind().unwrap_or(NodeKind::Project)`
   (`project_loader.rs:245`), which matches none of the per-kind attach
   loops — the node exists in the tree, drives nothing, and the only trace
   is a node status visible to Studio, never to serial. A corrupt project
   loads "successfully" minus arbitrary nodes.
2. **Flush abort.** `flush_registered_sinks` returned on the first failing
   sink; with `HashMap` iteration, one bad open killed a *random* subset of
   the remaining channels each frame — the host nondeterminism.
3. **Virtual driver pinned to `/rmt/ws281x0`.** The host stand-in claimed
   channel 0 for every open regardless of the manifest, so a second host
   channel could never open (`already claimed by virtual-ws281x-rmt0`) —
   a stand-in-divergence from the real S3 driver's per-channel claims.

**Fix** — Loader: `mark_node_load_error` now also emits
`log::warn!("ProjectLoader: node {label} did not load: …")` at both funnel
sites. Flush: every sink is attempted every frame; failures are logged per
sink with the endpoint named, and the first is returned after the loop.
Virtual driver: enumerates every manifest `/rmt/ws281xK` and claims
per-channel bundles like the real driver.

**Regression coverage** —
`lpa-server/tests/quad_output_channels.rs` (4 chains → 4 distinct open
endpoints with pairwise-distinct pixels; single chain → exactly one);
`project_loader.rs::a_node_whose_definition_does_not_parse_is_marked_failed`;
`engine_services.rs::a_failing_output_sink_does_not_suppress_the_others`
(fails 8/8 with the early return restored);
`hw_system.rs::virtual_ws281x_opens_one_output_per_declared_timing_resource`.

**Lesson** — "Loads successfully minus some nodes" is the worst possible
failure shape for authored content: every downstream symptom (a dark strip,
a missing channel) points at hardware, and the walk burned an hour on RMT
theories before the project file was suspected. A load error must be loud on
every surface the operator is actually watching — a status field only Studio
renders does not count on a serial console. Secondary lesson: partial
failure handling in fan-out loops (flush, attach) must contain per-item
errors, or one corrupt item nondeterministically shadows healthy ones.
