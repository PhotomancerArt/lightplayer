---
status: open
found: 2026-08-02      # how: hardware-walk
area: lpvm (Instance hot path) — lpvm-native, lpvm-wasm rt_wasmtime, rt_browser
class: untested-path
related:
  - ../adr/2026-08-01-float-mode-reaches-the-device.md
  - ../adr/2026-08-01-float-mode-as-a-compiler-parameter.md
  - ../design/float.md
---
# An f32 shader compiles on the device and then cannot render a frame

**Symptom** — on an ESP32-S3 (rev v0.2, MAC `d8:3b:da:47:29:70`) running the
shipping app image, a project whose shader node carries `"float_mode": "float"`
uploads, compiles, and *reports success* — and then fails on the first frame:

```
[shader-node] compilation succeeded (node=NodeId(10), elapsed=53ms,
  lpir_inst_count=160, final_inst_count=491, final_code_size=1964 bytes,
  float=hardware-f32)

LpServer::tick: Project Quad strips v3 tick error: Core("node NodeId(6):
  render control: control render: sample visual: sample visual: shader sample:
  sample_points_rgba16: render: call_render_samples `__render_samples_rgba16`:
  NativeJitInstance::call_render_samples requires FloatMode::Q32")

Error: deploy was acked, but the deployed project failed to run
```

The compile half is genuinely working — 491 Xtensa instructions of real
hardware-FPU code, `float=hardware-f32`, 68 bytes *smaller* than the same
shader's Q32 build. It is the execute half that refuses.

**Root cause** — the two hot-path entry points are Q32-only **by contract, in
every backend**, not by oversight in one:

| entry | native JIT | wasmtime | browser |
|---|---|---|---|
| `call_render_texture` | `instance.rs:624` | `instance.rs:549` | `instance.rs:461` |
| `call_render_samples` | `instance.rs:673` | `instance.rs:591` | `instance.rs:495` |

Each opens with `if float_mode != FloatMode::Q32 { return Unsupported }`. The
guards are correct: the trait's own documentation fixes the marshalling as
fixed-point — `points` "contains packed Q16.16 `[x, y]` pairs" and `out`
"receives packed RGBA16" (`lpvm/src/instance.rs:91-101`). There is no f32
marshalling for the frame boundary, so there is nothing for an f32 module to be
called *through*. `__render_texture_rgba16` / `__render_samples_rgba16` are the
only synthesised entries (`lpvm/src/lib.rs:189,221`); no f32 sibling exists.

So `float_mode` reaches the compiler, and the compiler does its job, but the
capability stops one layer short of the frame.

**Why the acceptance evidence missed it** — every f32 assertion we have goes
through a *different* door. The corpus, the 27/27 and 41/41 S3 silicon runs, and
the C6's 43/43 all call `call_q32` / `call_f32_words` / typed `LpvmInstance::call`
— direct entry points that marshal one value at a time. `call_render_samples` is
the sibling path none of them touch, and it is the only one the product uses per
frame. The 41/41 run specifically asked the app's own Q32-constructed engine for
Float per compile and asserted the module's disclosed `FloatImpl` — which is a
real check of the *compile* seam, and reads like a check of the whole thing.

**Fix** — none yet. The shape is an f32 frame boundary: either f32 variants of
the two synthesised entries with `f32`-packed `points`/`out`, or a documented
conversion at the boundary (Q16.16 in → f32 compute → RGBA16 out), which keeps
one marshalling contract and costs two conversions per sample. The second is
probably right for a first cut: the sample buffers are an interchange format
shared with fixtures and outputs, and widening them is a much larger change than
making the shader interior f32.

**Regression coverage** — none, and that is the actionable half of this entry. A
test that compiles *any* shader in `FloatMode::Float` and drives it through
`call_render_samples` would have failed the day M7 landed. The gap is not deep in
the backend; it is that no test ever asked an f32 module to render.

**Lesson** — *proving a capability through the door you built for testing does
not prove it through the door the product uses.* The f32 roadmap was unusually
disciplined about measurement — predictions-first, silicon everywhere, no
regenerated baselines — and it still shipped a capability that cannot be reached
by a running project, because every rig it built called modules directly while
the app calls them through a synthesised entry with its own ABI. When a feature's
acceptance evidence and its production caller use different entry points, the
evidence is about the entry point, not the feature. The cheap guard is to make at
least one acceptance test enter through the *product's* door, even when a direct
call is easier to assert on.

This also explains, structurally, why the roadmap's G3 found no f32-vs-Q32
performance number anywhere: it was not an oversight of measurement. There is
currently no way to run an f32 shader through a frame, so there was nothing to
time.
