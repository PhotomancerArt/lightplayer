---
status: fixed
found: 2026-08-03      # how: modules-vision GF walk — examples/meteor frozen in the editor sim while the clock advanced
area: lpvm-wasm rt_wasmtime + rt_browser (instance vmctx placement)
class: shared-mutable-state / silent-clobber
related:
  - lp-core/lpc-engine/tests/meteor_compute_animates.rs
  - lp-shader/lp-shader/src/tests.rs (compute_instances_on_one_engine_keep_isolated_state)
  - lp-shader/lpvm-native/src/rt_jit/module.rs (the backend that already did this right)
---
# Every wasm shader instance's vmctx sat at guest address 0, so coexisting shaders clobbered each other's uniforms and persistent globals

**Symptom** — `examples/meteor` opened in the Studio editor sim renders a
frozen frame: the Clock's seconds advance, the DECAY knob works, but the
meteors never move. The gallery card's GPU-tier preview of the same example
animates fine. Reproduced on the host with a bare
`ProjectLoader::load_from_root` + `Engine::tick` loop over
`TargetLpvmGraphics` — no studio involved
(`lp-core/lpc-engine/tests/meteor_compute_animates.rs`).

**Cause** — both wasm LPVM runtimes (`rt_wasmtime`, the host CPU tier, and
`rt_browser`, the editor sim's tier) passed a constant `0` as the vmctx
pointer (WASM param 0) on every guest entry and performed every host-side
vmctx access — `set_uniform`, `get_global`, fuel arm, trap read, globals
snapshot/reset — at absolute offsets from guest address 0 of the engine's
**one shared linear memory**. Emitted code addresses uniforms and globals
relative to the vmctx param (`lps-glsl` lowers every access as
`vmctx + offset`), so the pointer was fully relocatable — but with a fixed
base, every instance on the engine occupied the *same* block.

One live shader per engine never notices. Px shaders alone don't either:
they rewrite all their uniforms before every render call, so the clobber is
repaired just in time. The defect fires the moment an instance depends on
**state persisting between calls** — exactly what a `ComputeShader` node's
plain globals are documented to do. In meteor, each frame interleaves
`sim` (compute tick) and `render` (px render) on the same engine; the px
pass overwrote the compute instance's `prev_time`/`meteors` globals, the
compute shader reseeded from a self-consistent stale state, `dt` stayed 0,
and the positions pinned at their seed values. No error surfaces anywhere:
the readback even yields plausible-looking structs (with `dir` showing
another field's bytes through the layout mismatch).

**Why the tiers diverged** — on the GPU preview tier the render shader
compiles through naga/wgpu, leaving the compute shader as the *only* lpvm
instance, so nothing clobbered it and the gallery animated. The tier
difference was an artifact of instance count, not of any compute-path
difference — a trap worth remembering: "works on tier X" can mean "tier X
happens to run one fewer instance."

**Why only now** — `lpvm-native`'s `rt_jit` (the device backend) has
allocated a per-instance vmctx from the start
(`NativeJitModule::instantiate`), and until the modules vision landed there
was no shipped example whose correctness depended on cross-call state in a
wasm-tier instance. Meteor is the first.

**Fix** — mirror `rt_jit` in both wasm runtimes: allocate a per-instance
vmctx block (16-aligned, zeroed) at instantiation — from the engine's bump
region on wasmtime, from the app's own allocator on the browser — and
thread its base through every guest call's param 0 and every host-side
vmctx read/write. The legacy raw `render_frame` export (web-demo's entry
point) keeps its emitted vmctx=0 semantics; it is single-shader by
construction.

**Residual** —

- ~~The browser runtime still never calls `__shader_init` nor implements the
  globals snapshot/reset lifecycle that wasmtime and native have; a global
  with an initializer starts zeroed in the editor sim, and px globals are
  not reset per call there.~~ Fixed 2026-08-03: `rt_browser/instance.rs`
  now mirrors `rt_wasmtime` — `init_globals` (call `__shader_init` if
  present, then snapshot) at instantiation, and snapshot→globals reset
  before `call`/`call_q32`/`call_render_texture`/`call_render_samples`
  (compute ticks still persist state, as on the other tiers).
- `resolve_or_default_input` (`compute_shader_node.rs`) still swallows
  resolve errors into authored defaults with no node status — a failed
  `bus:time` resolve would freeze this same example again, silently, from
  a different cause.
