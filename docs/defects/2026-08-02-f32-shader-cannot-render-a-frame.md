---
status: fixed
found: 2026-08-02      # how: hardware-walk
fixed: this change
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

**Fix** — convert at the boundary, keeping **one** marshalling contract: the
frame boundary stays Q16.16 in / RGBA16 out in both modes, and the synthesised
wrapper decodes each coordinate into whatever an F32 *lane* means for the mode
it compiled in. `synthesise_render_texture` / `synthesise_render_samples_rgba16`
now take a `FloatMode`, and one `Q16CoordDecoder` emits:

- **Q32** — the lane *is* the Q16.16 word, so the decode stays the single
  `FfromI32Bits` reinterpret. Nothing is allocated and nothing is emitted for
  the hoisted scale, so Q32 IR is unchanged op for op and vreg for vreg — which
  is why no Q32 filetest snapshot moved.
- **Float** — `ItofS(word) * 2^-16` against one scale constant hoisted out of
  the render loop. Both steps are Guaranteed-class in `docs/design/float.md` §3
  (int→float correctly rounded, `2^-16` exactly representable), so the decode is
  bit-identical across f32 targets rather than target-defined.

The **out** side already worked: `FtoUnorm16` has had mode-aware lowerings since
M5, and `unorm_conv_f32` deliberately uses the same `floor(v * 65536)` clamped
convention as `unorm_conv_q32`, so the two modes agree to the count.

The alternative — f32-packed `points`/`out` variants of both synthesised entries
— was rejected for the reason the entry gave before the fix: those buffers are
an interchange format shared with fixtures and outputs, so widening them is a
far larger blast radius than making the shader interior f32. The cost paid
instead is two conversions per coordinate per sample, in the frame hot path.

With the marshalling correct, the guards came out of `lpvm-native`'s two
backends (`rt_jit` — the device — and `rt_emu`). The **wasm pair keeps
refusing, deliberately.**

> **Correction, 2026-08-02 (same day).** This paragraph originally said the wasm
> pair "cannot compile a correct Float module at all yet, because the wasm
> emitter's f32 builtin id resolution is unimplemented." **That reason is
> false**, and was false when written — it was inherited from
> `../adr/2026-08-01-float-mode-reaches-the-device.md`, whose own copy of the
> claim the f32 roadmap's G3 review had already identified as stale and left
> uncorrected. M5 (PR #224) added `resolve_builtin_id_for_mode` and threaded
> `float_mode` through the whole `lpvm-wasm` emit path. Measured on
> `d3ee69f09`: `wasm.f32` is **850/850 files, 6,345/6,345, 0 compile-fail**,
> including `@glsl` builtins (`builtins/trig-sin.glsl` 10/10) and `@lpfn`
> transliterations (`lpfn/` 89/89 across 14 files). If f32 builtin ids did not
> resolve, those would compile-fail.
>
> **The measured reason the guards stay:** removing them on `rt_wasmtime` and
> driving this entry's own tests produces *structurally correct* output that is
> uniformly **one count low** against the rv32-emulator oracle — 16383 for
> 16384, 32767 for 32768, 8191 for 8192, every channel, both entries. That is
> the known wasmtime last-bit divergence
> (`2026-07-30-q32-native-vs-wasmtime-last-bit.md`), which is exactly why
> `rt_emu` and not wasmtime is the host oracle. It is a *numeric agreement*
> question, not a capability one, and it is now the thing to decide: classify
> the one count under `../design/float.md` (Guaranteed → fix it; Unspecified →
> drop the guard and stop asserting cross-backend equality at this boundary).
>
> `rt_browser` was **not** measured — it runs in the browser's own wasm engine,
> so its agreement is *unverified* rather than known-bad. Refusing there is the
> conservative read of an unmeasured tier, and Float still previews on the GPU
> tier.

So the three-way split is a decision with one open follow-up, not an accident —
but the follow-up is "classify one count", not "implement a lowering".

> **Amendment, 2026-08-07 — the follow-up resolved, and the correction above is
> itself corrected.** The three-way split is gone: both wasm guards are lifted,
> and all four backends now run the frame entries in both modes.
>
> The correction block above says the one count "is the known wasmtime last-bit
> divergence … a *numeric agreement* question, not a capability one, and it is
> now the thing to decide". **That attribution was wrong.** Measured on
> 2026-08-07 by driving this defect's own product-door test through
> `WasmLpvmEngine`: `lpvm-wasm`'s inline `FloatMode::F32` lowering of the unorm
> ops used the GPU `v * 65535` scale instead of the `floor(v * 65536)` clamped
> convention `docs/design/float.md` §7 fixes for the frame boundary and
> `unorm_conv_f32` implements — a **Guaranteed**-class violation with a
> one-line cause, not a target-defined rounding difference. Full writeup:
> `2026-08-07-wasm-f32-unorm-scale-convention.md`.
>
> With the scale corrected the wasm f32 frame path is **bit-identical** to the
> rv32 oracle on this file's shared table, so the guard's own decision rule
> ("Guaranteed → fix it") is what lifted `rt_wasmtime`'s pair. `rt_browser`'s
> pair went with them: it shares the emitter, the fix is in the emitter, and the
> refusal's stated reason was the wasmtime measurement it was reasoning by
> analogy from.
>
> **Regression coverage added:** `lps-filetests/tests/f32_render_entry_wasm.rs`,
> the wasm sibling of this entry's `f32_render_entry.rs`, asserting the same
> table exactly through the same two product calls. Which is the lesson below,
> applied twice: the product's-door test caught this the first time it was
> pointed at the engine the *host* uses, having been written for the engine the
> *device* uses.

**Regression coverage** — `lp-shader/lps-filetests/tests/f32_render_entry.rs`,
entering through the **product's** door: it compiles GLSL with
`LpsEngine::compile_px_desc(...).with_float_mode(Float)` and drives
`sample_points_rgba16` and `render_frame` — the same two calls the app makes per
frame — on the host rv32 emulator. Verified load-bearing by reverting just the
Float decode: both f32 tests fail with every channel at 0 (the "renders black"
signature), both Q32 controls still pass. `lp-shader`'s synth unit tests pin the
same contract at the IR level, including that Q32 gains no op.

**Verified on the board that found it** — same ESP32-S3 (rev v0.2, MAC
`d8:3b:da:47:29:70`), same `quad-strips-v3` project, `float_mode` flipped
between `float` and `fixed`:

| build | compile | frames |
|---|---|---|
| `float` | `float=hardware-f32`, 502 inst / 2,008 B, 52 ms | **fps=29, tick=32ms** |
| `fixed` (control) | `float=fixed`, 508 inst / 2,032 B, 48 ms | fps=29, tick=32ms |

The f32 build renders; the Q32 build is not slower.

> **Note, 2026-08-08 — do not generalize that second clause.** "The Q32 build
> is not slower" is true of *this* fixture and is the origin of the
> "the S3 is fps-neutral" belief. `quad-strips-v3` is a small fixture and the
> 2026-08-07 dome-scale bench falsified the generalization: at 1500 LEDs the
> S3 pays the same ~20% f32 penalty as the classic, dominated by FPU
> dependent-chain latency in the shader interior. Measured decomposition:
> `../design/float.md` §4.

One residual per-project
`tick error` remains in **both** modes and is unrelated: `quad-strips-v3`
names classic-ESP32 output endpoints (`ws281x:rmt:IO18` and friends) the S3
does not have. It was always there — the shader error simply fired first and
masked it.

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
