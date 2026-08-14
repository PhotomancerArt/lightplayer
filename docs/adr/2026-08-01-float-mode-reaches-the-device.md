# ADR: Float mode is chosen per shader node and carried per compile

- **Status:** Accepted
- **Date:** 2026-08-01
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None. **Amended 2026-08-08** by
  `2026-08-08-float-semantics-per-target-representation.md`, in two places
  marked in the text below: decision 1's framing of the slot as a per-node
  *semantics* choice (the slot survives; its meaning demotes to an optional
  pin, and absence — Auto — is now the normal state), and the second
  consequence's claim that the wasm CPU preview tier refuses Float (it no
  longer does, and the stated reason was a misdiagnosis). Decisions 2–6 and
  every measurement here stand unchanged.
- **Builds on:** `2026-08-01-float-mode-as-a-compiler-parameter.md` (float mode
  is an emitter parameter; hardware capability is a property of the target and
  the result is reported, not requested) and
  `2026-07-09-preview-fidelity-tiers.md` §4 (tier selection is explicit and
  never a silent fallback).

## Context

M7 linked native f32 into the ESP32-S3 image and proved it on silicon, 27/27.
Nothing could ask for it. `lp-gfx-lpvm`'s `TargetLpvmGraphics::new` built the
engine with `NativeCompileOptions::default()` — Q32 — and M2's `float_mode`
slot reached exactly one thing: the recompile latch. The S3 paid 65,680 B for
a path no shader could enter, and the prior ADR named plumbing it as the first
follow-up.

Two facts shaped the answer, and both correct assumptions that were in the air:

**The slot is per shader node, not per project.** M2 put `float_mode` on
`ShaderDef` and `ComputeShaderDef`, where it is authored per node
(`"float_mode": "fixed"` in each shader's JSON). The prior ADR called it
"project-level"; that was loose. Keeping it where M2 put it is a decision
(below), not an accident.

**The graphics backend is constructed once, at boot, before any project
loads.** `fw-esp32s3`'s `main` builds one `TargetLpvmGraphics`, and every
project that ever runs on that boot compiles through it. So a mode chosen at
engine construction cannot express a per-node choice, and rebuilding the engine
to change modes is not available: the engine owns the shared memory arena that
every live texture handle points into.

## Decision

### 1. The mode stays on the shader node

A project may mix Fixed and Float shaders. That is the finest granularity, so
it is the one that can express the others later: a project-level default that
nodes inherit, or a deploy-time capability negotiation, both need per-node
storage underneath, and neither can be added without it. Moving the slot to
`ProjectDef` would also cost a `PROJECT_FORMAT_VERSION` bump and refusal of
every existing artifact (alpha posture: bump and refuse, never migrate) in
exchange for a coarser answer.

> **⚠️ Dated amendment — 2026-08-08.** The *placement* decided here stands: the
> slot is still on `ShaderDef` / `ComputeShaderDef`, still per node. Its
> **meaning** is superseded by
> `2026-08-08-float-semantics-per-target-representation.md`.
>
> This section reasons about "a project may mix Fixed and Float shaders" as
> though the two were peer *semantics* an author chooses between. They are not.
> Float is the product's one authored semantics; Q32 is an execution
> representation the target picks. The slot is therefore an optional **pin** —
> `OptionSlot<ValueSlot<FloatMode>>`, where **absence (Auto) is the normal
> state** and resolves to the target's native representation. Format v5→v6
> deleted the spelled-out `"float_mode": "fixed"` from every committed project
> for exactly that reason: it was the pre-posture default written out, a pin
> that pins nothing.
>
> The "alpha posture: bump and refuse, never migrate" clause above is also
> stale as of `2026-08-04-project-format-migration-architecture.md` — the
> product migrates now (`lpa-upgrade`), which is what made the demotion cheap.
> The paragraph is left in place so the record shows the constraint that was
> real on 2026-08-01.

### 2. Float mode is a **per-compile** parameter of the engine, not per engine

`LpvmEngine` grows `LpvmCompileParams { config, float_mode }`, replacing the
`CompilerConfig`-only `compile_with_config`. `NativeJitEngine` reads
`params.float_mode` into the `NativeCompileOptions` it hands `compile_module`,
so one engine compiles Fixed and Float modules side by side, per node, with no
second arena and no second engine.

The struct rather than a second argument because the two travel together at
every call site, and because `CompilerConfig` — inlining and texture bounds —
is a middle-end concern that float mode explicitly is not. Putting `float_mode`
*into* `CompilerConfig` was rejected for that reason: it would push the mode
into every backend's middle end and contradict the prior ADR's placement of it
on the emitter's options.

### 3. Capability is asked, not discovered by failing

`LpvmEngine::supports_float_mode(mode)` is a query callers make **before**
compiling. Its default admits Q32 only, which is exactly what the default
`compile_with_params` (ignore the params, delegate to `compile`) can honour —
an engine that widens one must widen the other, and its doc says so.

Three answers exist in the tree, and the asymmetry is the point:

| Engine | Answer | Why |
|---|---|---|
| `NativeJitEngine` (rt_jit) | Q32 always; F32 iff `IsaTarget::native().f32_lowering() != Unsupported` | Reads the float-capability seam, so the answer is true per *target*, not per build |
| `NativeEmuEngine`, `EmuEngine` | the mode it was constructed with | They ignore per-call params; claiming more would be a lie |
| wasm `rt_wasmtime`, `rt_browser` | the mode it was constructed with | Same, and `WasmOptions::default()` is Q32 |

Asking first is what makes the failure legible. Without it, a Float request on
a C6 runs the whole GLSL frontend and then dies in lowering with *"float_mode=f32
needs the `float-f32` feature on lpvm-native"* — true, and useless to the person
who set a dropdown.

### 4. `ShaderSemantics` gains `F32Cpu`, and every backend states two tiers

The request travels above the compiler as a semantics tier, in the field that
already exists for it. `LpGraphics` gains `float_semantics()` — the sibling of
`native_semantics()`, answering the same question for an authored `Float`. The
node picks between them; it does not own a mapping table.

`F32Cpu` is deliberately **not** folded into `F32Gpu`. Both are IEEE f32 and
they are not the same contract: the GPU tier carries documented divergence
latitude, and the CPU tier is held to `docs/design/float.md` exactly. The CPU
backend refuses an explicit `F32Gpu` request rather than treating it as a
synonym, because accepting it would silently answer a fidelity question the
caller asked a different backend.

The default `float_semantics()` is `native_semantics()`, and that is correct
rather than lazy for the two backends that take it: the GPU tier runs IEEE f32
whichever mode was authored — its latitude, not a dropped request — and
`NullGraphics` compiles nothing, so its answer is unreachable.

### 5. The refusal surfaces as the node's compile error, today

`LpvmGraphics::compile_shader` refuses with a message naming the backend that
refused and the slot the author can change. It rides the existing
keep-last-good path to `NodeRuntimeStatus::Error`, which Studio already shows
on the node, and the output clears to black rather than rendering in the wrong
numerics.

That is the **backstop**, not the intended UX. `float_mode` is reachable today
only through the shader card's advanced drawer — the permanent face
(`UiShaderFace`) does not offer it — so the intended shape is: the face gates
the choice on what the target board can actually do, and a user who bypasses
the face and sets it in advanced gets this compile error. The gating needs the
firmware manifest, which does not exist yet; the backstop has to exist either
way, and it exists now.

Not chosen: warn and compile Fixed. A board quietly given different numerics
than the author asked for is the failure class this repo refuses
(`2026-07-09-preview-fidelity-tiers.md` §4, and decision 3 of the prior ADR).

### 6. The ESP32-C6 stays Fixed-only

The C6 *could* run Float — `Rv32imac` answers `SoftFloatCalls`, and M9's
soft-float path resolves to the chip's ROM `rvfplib`. It does not, because
`float-f32` stays off in its image: no FPU, a 3 MB app partition it has
overrun twice, and soft float on a 160 MHz core without one is unlikely to hold
a frame budget. "No FPU" is therefore not the reason the C6 refuses; a
flash-budget decision is. Its byte-identical image across this change remains
the negative control that proves the feature gate holds.

## Consequences

- **A Float shader runs in hardware f32 on an ESP32-S3 app image**, chosen per
  node, with no rebuild and no flag. The linked capability is reachable.
- **The wasm CPU preview tier refuses Float**, because its engine is built Q32
  and its f32 emit path still resolves `@lpfn`/`@glsl` imports to Q32 builtin
  ids — the same defect that keeps `wasm.f32` out of `DEFAULT_TARGETS`. A Float
  shader previews on the **GPU** tier normally; on the CPU preview tier it
  shows the compile error. This is a real gap in authoring, recorded as a
  follow-up rather than papered over: an invalid wasm module would have been a
  worse failure than a clear refusal.

  > **⚠️ Dated correction — 2026-08-02. The second clause above is false.** The
  > decision stands; its stated reason does not, and the text is left in place
  > rather than rewritten so the record shows what was believed on 2026-08-01.
  >
  > The f32 emit path does **not** resolve `@lpfn`/`@glsl` imports to Q32
  > builtin ids. M5 (PR #224) added `resolve_builtin_id_for_mode` and threaded
  > `float_mode` through `lpvm-wasm`'s emit path. Measured on `d3ee69f09`:
  > `wasm.f32` runs **850/850 files, 6,345/6,345, 0 compile-fail**, including
  > `@glsl` (`builtins/trig-sin.glsl` 10/10) and `@lpfn` (`lpfn/` 89/89).
  > `wasm.f32`'s absence from `DEFAULT_TARGETS` had the same stale reason and
  > was a CI-cost question, not a correctness one.
  >
  > **The first clause is the true and sufficient one:** the tier's engine is
  > built Q32, and `supports_float_mode` answers "the mode I was built with".
  > On the *frame* boundary there is now a second, measured reason on
  > `rt_wasmtime` — with its guard removed it renders one count low against the
  > rv32 oracle, the known wasmtime last-bit divergence
  > (`../defects/2026-07-30-q32-native-vs-wasmtime-last-bit.md`). The browser
  > tier is unverified rather than known-bad.
  >
  > **Why this note exists at all.** The f32 roadmap's G3 review found this
  > clause stale and deliberately left it, judging a silent amendment worse than
  > a note in the review artifact. The clause was then cited *from this ADR* by
  > PR #287 into live source comments and a user-facing Studio error string. An
  > uncorrected ADR is not inert — it is what the next implementer quotes. Hence
  > a dated correction in place, which is the form that was always available.

  > **⚠️ Dated correction — 2026-08-08. The consequence is now false in full,
  > and so is the correction above it.** The wasm CPU preview tier **renders
  > Float**. Both `FloatMode::Q32` guards are gone from `call_render_texture`
  > and `call_render_samples` on `rt_wasmtime` and `rt_browser`; both engines
  > honour a per-compile `float_mode` the way `NativeJitEngine` does; and the
  > instance reads its mode from the compiled module rather than the engine's
  > construction-time options, which also fixed `FloatImpl` silently disclosing
  > `fixed` for a wasm F32 module.
  >
  > The 2026-08-02 block above says the surviving reason on the frame boundary
  > is "the known wasmtime last-bit divergence". **That attribution was wrong
  > too.** `lpvm-wasm`'s inline `FloatMode::F32` lowering of the unorm ops used
  > the GPU `v × 65535` scale instead of the product's `floor(v × 65536)`
  > clamped convention (`../design/float.md` §7) — a **Guaranteed**-class bug
  > with a one-line cause, not a target-defined rounding difference. The
  > uniformity was the tell: a rounding divergence is sparse, a scale error is
  > uniform, and every channel was off by exactly one count. Fixed, the wasm f32
  > frame path is **bit-identical** to the rv32 oracle
  > (`../defects/2026-08-07-wasm-f32-unorm-scale-convention.md`,
  > `lps-filetests/tests/f32_render_entry_wasm.rs`).
  >
  > So the correct reading of this consequence's history is that a
  > *capability* claim was replaced by a *numerics* claim, and both were
  > misdiagnoses of one emit bug that no filetest could reach — the corpus ran
  > 6353/6353 on `wasm.f32` throughout. `rt_emu` remains the host oracle and the
  > `wasm.q32` last-bit divergence is untouched. Posture and full context:
  > `2026-08-08-float-semantics-per-target-representation.md` §5.
- **A stub backend must state its tiers.** `CountingGraphics` in `lpc-engine`'s
  tests inherited the one-tier default and answered Q32 for a Float request —
  caught by the new test, which is the same bug shape the tier request exists
  to prevent. Test doubles forward `float_semantics()` alongside
  `glsl_frontend()`.
- **Compute shaders carry the mode too**, straight on `CompileComputeDesc`;
  they have no `ShaderSemantics` tier because they only ever run on a CPU
  backend.
- **Both images grew, and the ESP32-C6's zero-delta control does not apply
  here.** That control is about the `float-f32` *feature gate* not leaking; this
  change is not behind that gate, so it is always-linked code on every board.
  Measured against the branch point (`1367ccceb`), same recipe:

  | Image | Before | After | Δ |
  |---|---|---|---|
  | ESP32-C6 | 2,874,528 B | 2,874,944 B | **+416 B** |
  | ESP32-S3 | 1,826,352 B | 1,826,608 B | **+256 B** |

  The C6's 416 B splits cleanly, measured by building it once with the
  capability pre-checks and their message text removed and the mode threading
  kept: **160 B is the threading**, **256 B is the legible refusal** — the
  strings and the branches that turn "this fails somewhere in lowering" into a
  message naming the backend and the slot. That is the price of §4, stated
  rather than absorbed. The first attempt cost 960 B; two thirds of that was
  `{:?}`, which is why `ShaderSemantics::name()` and `FloatMode::as_str()`
  exist.

  Both numbers are against 271 KB (C6) and 4.4 MB (S3) of headroom. If the C6
  ever needs the 256 B back, dropping its pre-check is the lever — it would
  still refuse, just with the lowering's Cargo-feature message.

- **Proved on silicon**, esp32s3 rev v0.2: `passed=41 failed=0`, up from 27.
  The 14 new cases are a Q32-constructed engine — the one the app boots — asked
  for Float per compile, asserting the module's disclosed `FloatImpl` and the
  same goldens the per-engine route uses.

## Alternatives Considered

- **Float mode on `lpir::CompilerConfig`.** Rejected: `CompilerConfig` is the
  middle end (inlining, texture bounds), and the prior ADR placed float mode on
  the emitter's options for a reason. It would also reach every backend's
  middle end for a decision only the emitter acts on.
- **One `F32` semantics variant covering CPU and GPU.** Rejected: different
  fidelity contracts. A single variant would make "which divergence bounds
  apply" unanswerable from the request.
- **Two engines, one per mode, in `LpvmGraphics`.** Rejected: each engine owns
  a shared memory arena, and doubling it on a device to express a per-node
  choice is the wrong trade by a wide margin.
- **Mapping `FloatMode → ShaderSemantics` in the shader node.** Rejected in
  favour of `float_semantics()` on the backend: which tier a backend runs for
  Fixed and for Float is a per-backend product decision, and the node is the
  wrong place to know that the GPU tier ignores the distinction.
- **Letting the wasm engines claim F32.** Rejected: they would compile a module
  whose builtin imports point at the wrong family, which fails later and less
  clearly than refusing.

## Follow-ups

- **Gate the choice on the shader card's face** once the firmware manifest can
  report a board's float capability, so Float is not offered where it cannot
  run. The compile error stays as the advanced-drawer backstop.
- ~~**Fix `lpvm-wasm`'s f32 builtin id resolution**, which unblocks both the CPU
  preview tier and `wasm.f32` as a default filetest target.~~ — **closed
  2026-08-08, and it was never the blocker.** The resolution was already
  implemented (M5, PR #224); `wasm.f32` joined `DEFAULT_TARGETS` 2026-08-02.
  What actually blocked the CPU preview tier was the unorm scale bug in the
  same crate's emit path — see the 2026-08-08 correction above.
- ~~**A project-level default** that nodes inherit, if authoring a mixed-mode
  project turns out to be the common case rather than the exception.~~ —
  **dissolved 2026-08-08.** With the slot demoted to an optional pin whose
  absence means the target's native representation, the common case carries no
  value for a project default to supply.
