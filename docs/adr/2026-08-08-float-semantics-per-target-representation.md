# ADR: Float is the semantics; Q32 is a per-target representation

- **Status:** Accepted
- **Date:** 2026-08-08
- **Deciders:** Photomancer (Yona, ruling D7, 2026-08-07)
- **Supersedes:** None outright. **Amends**, with dated in-place notes in each:
  `2026-08-01-float-mode-reaches-the-device.md` (decision 1's framing of the
  slot as a per-node *semantics* choice — the slot survives, its meaning
  demotes to a pin; and its second consequence, the wasm tier's refusal, whose
  stated reason was a misdiagnosis) and
  `2026-08-01-float-mode-as-a-compiler-parameter.md` (its third follow-up, the
  wasm CPU preview tier, now closed). `2026-07-09-preview-fidelity-tiers.md`
  needs no further retirement — decision 1 there was already superseded
  2026-08-01, and this ADR **generalizes** its GPU-latitude clause rather than
  retiring anything new.
- **Builds on:** `2026-08-01-float-mode-as-a-compiler-parameter.md` (float mode
  is an emitter parameter, and a runtime-matched value is why LTO cannot drop
  the arms a board never enters — the *why* of making the mode set a build
  property), `docs/design/float.md` (normative f32 semantics, including §4's
  measured perf decomposition and §7's frame boundary), `docs/design/q32.md`.

## Context

The product's numeric architecture grew backwards from its market. Q32
(Q16.16 fixed point) was built as the prime path when the only target was the
FPU-less ESP32-C6. The customer base then moved to classic-Xtensa hardware —
every QuinLED digital board — and the forward chip line (ESP32-S31) is
RV32IMAFC with a per-core FPU. f32 now exists end to end: compiler, builtins,
both Xtensa emulators, silicon conformance on LX6 and LX7, frame rendering
since PR #287, classic default-on since PR #372.

It was nonetheless bolted on as a **per-shader-node authored choice**. A new
shader's JSON carried `"float_mode": "fixed"`, which read as the author
picking one of two peer numeric worlds — and the choice meant different things
on different tiers (the GPU tier has ignored an authored `Fixed` and rendered
IEEE f32 since 2026-07-09, by documented latitude). Two events in the same
week forced the question:

**The bench decomposition (2026-08-07).** Same project
(`projects/test/zook-dome-1500`, 5×300 LEDs), same firmware image, only
`float_mode` flipped, with a trivial-interior control shader isolating the
frame-boundary seam from the shader interior. f32 costs **~19–20% at dome
scale on both LX6 and LX7**, and the cost is dominated by FPU dependent-chain
latency *inside the shader*, not by the boundary — the boundary seam is ~2 ms
per 1500 samples, of which only the input half is even theoretically
ABI-recoverable. The full table and the derivation are normative in
`docs/design/float.md` §4 and are not restated here. Two prior beliefs died
with it: the PR #372 attribution of the slowdown to frame-boundary conversions
is **falsified** (the trivial control pays the same boundary cost and keeps
only ~2 of the classic's 6 ms), and "the S3 is fps-neutral" was a
small-fixture artifact — at dome scale the LX7 pays the same ratio as the LX6.

**The preview refusal.** A shader pinned to Float could not render in the
Studio CPU preview at all: the wasm frame entry points refused
`FloatMode::F32`. That is a shader the author can create and cannot see.

The two combine into one architectural question. If f32 is slower on the
hardware the entire customer base owns, but float is what shaders are actually
written in and what every forward target executes natively, then *semantics*
and *representation* are not the same axis and must stop sharing a slot.

## Decision

### 1. Float is the product's one authored semantics

Shaders are authored in float. GLSL is already the canonical authoring
language and its numbers are real numbers; the author reasons about `0.5`, not
about `32768` in a Q16.16 word.

**Q32 stops being a peer semantics and becomes an execution representation** —
the fast one on Xtensa, the only one on FPU-less chips. This is not a new
mechanism. `docs/design/float.md`'s Guaranteed / Target-defined / Unspecified
classes already make per-target numeric divergence legal, and the GPU tier's
documented latitude is the precedent: `2026-07-09-preview-fidelity-tiers.md`
established that an authored `Fixed` renders IEEE f32 on the GPU tier and that
this is a product decision, not a defect. This ADR **generalizes that
latitude** from one tier to the general rule.

`docs/design/q32.md` stays normative for what Q32 *is*. What changes is its
standing in the product: an implementation of float semantics on targets that
execute it faster, not a second thing to author against.

### 2. `float_mode` demotes to an optional pin, and absence is the normal state

`ShaderDef.float_mode` and `ComputeShaderDef.float_mode` are now
`OptionSlot<ValueSlot<FloatMode>>`
(`lp-core/lpc-model/src/nodes/shader/{shader_def.rs, compute_shader_def.rs}`).
**None = Auto = the target's native representation**, and an unpinned shader
simply has no `float_mode` key in its JSON.

Auto is the *slot* being unset, deliberately **not** a third `FloatMode`
variant. The compiler must always receive a concrete mode; a variant meaning
"decide later" would leak an undecidable value into every emitter and every
`match` below the engine. The resolution happens in exactly one place
(`lpc-engine/src/nodes/shader/shader_node.rs` `semantics_for`, and the compute
sibling in `shader_abi/compute_desc.rs`):

| slot | semantics tier |
|---|---|
| `None` (Auto) | `graphics.native_semantics()` |
| `Some(Fixed)` | `graphics.native_semantics()` — an alias on every shipping backend, all of which are Q32-native |
| `Some(Float)` | `graphics.float_semantics()` |

Pinned `Fixed` aliasing native is true *today* and stops being true on the
first f32-native image; that is deferred below rather than pre-solved.

Format **v5 → v6** (`PROJECT_FORMAT_VERSION = 6`,
`lp-app/lpa-upgrade/src/steps/v5_to_v6.rs`) deletes the spelled-out
`"float_mode": "fixed"` from every shader and compute-shader node — the
pre-posture default, a pin that pins nothing — and passes `"float"` through
untouched, because that one is a real request. The migration is
behavior-preserving *because* every current backend is Q32-native: the same
shader compiles to the same code on the same board before and after. All 70
committed projects were migrated with the real tool; the repo carries no
`"float_mode": "fixed"` anywhere.

In the editor the pin lives where it already lived — the advanced drawer,
never the shader card's permanent face — and now renders as the codebase's
standard **unset-option row with a `+` to pin**, not as a three-item dropdown.
That is the honest shape: Auto is the absence of a choice, so the UI shows an
absence, and the row's description carries what a dropdown label could not
("Unset = Auto (target default) … Set this only to force one representation
regardless of target").

### 3. The default is chosen by measurement, not by FPU presence — and the
### choice is falsifiable

| target | image carries | native default | why |
|---|---|---|---|
| ESP32-C6 | Q32 only | Q32 | no FPU; soft float unviable at frame rate |
| classic ESP32 (all QuinLED) | Q32 + f32 | **Q32** | Q32 ~19% faster on a real shader at dome scale |
| ESP32-S3 | Q32 + f32 | **Q32** | same ~20% ratio at dome scale — the neutral datapoint was small-fixture |
| ESP32-S31 / RV32F | f32 (goal) | f32 | standard FPU; measure when silicon arrives |
| host / Studio / GPU | all | per previewed target | preview honesty |

**"Has an FPU ⇒ f32-native" is wrong**, and it is wrong for the entire current
customer base rather than for one outlier chip. The S3 has a real FPU and
still pays ~20% at dome scale. The Xtensa default is Q32 because 20% at the
edge of choppy is real on the boards people own — not because of anything
about the silicon's capability.

That reasoning is falsifiable, so the flip condition is written down rather
than left as a vibe. **If** the software-pipelining / 2-sample-interleave
spike in the synth loop brings f32 within a few percent of Q32 on Xtensa
(the dominant cost is dependent-chain latency, which is exactly what
interleaving hides), **or** frame-floor work buys enough dome-scale headroom
that the mode delta stops mattering — the classic's trivial-fixed floor is
28–29 ms at 1500 LEDs, ≈4,500 cycles/sample of frame-path machinery that
dwarfs every mode delta — **then the Xtensa default flips to f32 and Q32
begins its sunset.**

This is **not a one-way door**, by construction: the default is a firmware
build constant plus a Studio default. It is never encoded in a project. A
project authored today contains no representation choice at all, so flipping
the default reinterprets every existing project for free.

### 4. A pin requests; the target executes in the nearest mode it carries;
### refusals stay loud

The GPU tier's latitude generalizes: a pin is a request against the target's
repertoire, and a target may legitimately execute it in the nearest
representation it has. That is **representation latitude**.

It is not a licence to fall back on missing capability.
`2026-07-09-preview-fidelity-tiers.md` §4 ("tier selection is always explicit
and visible — never a silent fallback") is about **capability refusal**, and
it stands unchanged and unweakened. A board whose image linked no f32 backend
still *errors* on a pinned Float — it does not quietly compile Q32 — because a
board given different numerics than the author asked for is the failure class
this repo refuses. `LpvmEngine::supports_float_mode` is still asked before
compiling, and the refusal still names the backend and the slot.

The distinction is worth stating precisely, because the two look alike from
outside: **latitude is a target answering a request in the representation it
carries; fallback is a target silently answering a request it cannot serve.**
The first is documented per target. The second is a defect.

### 5. Every faithful preview tier runs the target's representation — and the
### wasm tier's refusal was a bug, not a classification

A Float shader must render in the Studio CPU preview. As of this work it does:
the `FloatMode::Q32` guards are gone from `call_render_texture` and
`call_render_samples` on both `rt_wasmtime` and `rt_browser`, both wasm
engines honour a per-compile `float_mode` (the pattern `NativeJitEngine`
already used), and the compiled module's mode — not the engine's
construction-time option — is the instance's source of truth, so
`FloatImpl` disclosure stopped silently reading `fixed`.

**The refusal's stated reason did not survive being investigated.** It had
been recorded as a *numeric agreement* question: with the guards removed the
wasm f32 frame path read uniformly one count low against the rv32 oracle, and
that was attributed to the known wasmtime last-bit divergence
(`docs/defects/2026-07-30-q32-native-vs-wasmtime-last-bit.md`). The plan of
record was to classify that one count under `float.md` and accept it as
Target-defined.

It was neither. `lpvm-wasm`'s inline `FloatMode::F32` lowering of the four
unorm ops used the **GPU convention** (`v × 65535`) where every other tier
uses the product's `floor(v × 65536)` clamped convention that `float.md` §7
fixes for the frame boundary — a **Guaranteed**-class violation with a
one-line cause. The uniformity was the tell, and it was visible in the first
measurement: a rounding divergence is sparse, a scale error is uniform. Fixed,
the wasm f32 frame path is **bit-identical to the rv32 oracle**, so the
guard's own decision rule ("Guaranteed → fix it") is what lifted it. Full
writeup: `docs/defects/2026-08-07-wasm-f32-unorm-scale-convention.md`.

There is therefore **no Target-defined acceptance here to record**, and this
ADR does not create one. `rt_emu`, not wasmtime, remains the host oracle; the
`wasm.q32` last-bit divergence is untouched and still a defect.

Two structural lessons are worth carrying, because both are about how this
family fails:

- **An inline lowering that mirrors a library function is a second
  implementation**, and both having green tests is the condition under which
  they drift. `LpirOp::FtoUnorm16` is emitted only by the synthesised frame
  wrappers, which no filetest drives — the corpus ran 6353/6353 across the
  whole period the scale was wrong.
- **When a new symptom is claimed by an existing defect, ask what the existing
  defect predicts about the *distribution*, not just the magnitude.** This
  misdiagnosis survived four months of restatement — into a source comment, a
  defect doc, an ADR consequence, and a plan — because nobody checked the
  shape.

## Consequences

- **A new shader has no numeric choice to make.** It is float-semantic and
  runs whatever the target is native in. The pin exists for the author who
  needs to escape Q16.16's `±32768` / `1.5e-5` range ceiling on a board where
  Q32 is the default — a ceiling that is confirmed not currently biting
  authors, which is why it does not drive the architecture.
- **Auto is behaviourally identical to the old default-Fixed on every current
  backend** — device JIT, `rt_emu`, wasm preview, and the GPU tier (which
  already ignored `Fixed`). That was the load-bearing invariant of the model
  change and it held: nothing drifted beyond the schema tree and the
  intentionally migrated project files.
- **The format took v6.** The next format bump in flight (the WLED pattern
  work's planned v5→v6) renumbers to v7.
- **`docs/design/float.md` is the normative home for the perf story**, §4 for
  the measured decomposition and §7 for the frame boundary. This ADR
  summarizes and points; it does not carry a second copy of the table, because
  a second copy is how the falsified attribution propagated in the first
  place.
- **The build-time mode-set mechanism is identified but not yet built.**
  `2026-08-01-float-mode-as-a-compiler-parameter.md` measured that a
  runtime-matched `FloatMode` keeps LTO from dropping unused arms; making each
  image's mode set a compile-time constant is what would return that flash.
  Today's images still carry both arms on classic and S3 (+63 KB, PR #372) —
  cheap enough to defer, and the `float-f32` feature already models the
  mechanism.
- **The classic's real ceiling is the frame path, not the mode.** 28–29 ms per
  1500 LEDs with a two-op shader body is where dome-scale performance actually
  lives. Recorded here because it is the context that makes the Q32-default
  decision proportionate, and because it carries half the flip condition.

## Deferred, consciously (the S31 era)

Each of these is unanswerable without f32-native silicon in hand, and each is
cheap to add later precisely because nothing about it is encoded in projects:

- **Single-mode images** — an S31 image that carries f32 only, and whether it
  carries Q32 at all (lean: no).
- **A per-image native-default constant, reported through the firmware
  manifest**, so Studio can show and gate the pin against what the board
  actually carries.
- **An f32-native points-in frame ABI.** Worth ~1 ms per 1500 LEDs per §4's
  decomposition, and it deletes the Float arm of `Q16CoordDecoder` on such an
  image. Not worth a classic/S3 firmware churn on its own — ~3% on boards that
  default to Q32 anyway. The output side stays integer in every design.
- **Per-device preview mode selection** in a multi-device Studio: which
  target's representation the CPU preview should run when several are
  attached.
- **What pinned `Fixed` means on an f32-native image.** Today it aliases
  native. On the first image where it does not, it becomes a real request that
  the target may or may not carry — decision 4's rule applies, but which
  answer that image gives is a decision that image's ADR makes.

## Alternatives Considered

- **Flip the Xtensa default to f32 now, on the strength of "float is the
  semantics".** Rejected on measurement: ~20% at dome scale on the boards the
  entire customer base owns, at a frame rate already near choppy. The posture
  does not require the default to follow it immediately — separating semantics
  from representation is exactly what lets the default lag the direction.
- **Keep `float_mode` a required two-way authored choice.** Rejected: it asks
  every author a question with no user-facing meaning, in a slot whose answer
  already differed by tier. It also freezes the answer into project files,
  which would make the flip condition a migration instead of a constant.
- **Add `Auto` as a third `FloatMode` variant.** Rejected: the compiler must
  receive a concrete mode, so an undecidable variant would have to be matched
  and rejected at every emitter boundary. Absence in an `OptionSlot` expresses
  the same thing where it is already unrepresentable-wrong to forget it.
- **A project-level `float_mode` default that nodes inherit.** Rejected as
  answering the wrong question: with Auto the common case has no pin at all,
  so there is nothing for a project default to save.
- **Accept the wasm one-count divergence as Target-defined and lift the guards
  on that basis** — the plan of record before P2. Rejected by the evidence: it
  was a Guaranteed-class scale bug. Accepting it would have written a false
  Target-defined row into `float.md` and permanently forbidden a cross-target
  assertion that is in fact exact.
- **Widen the frame ABI to f32 now** so both modes share one interior
  representation end to end. Rejected on cost/benefit: ~1 ms of the ~2 ms seam
  is recoverable and only on the input side, against a firmware ABI churn on
  boards that default to Q32. It is the right change on the first f32-native
  image and is deferred to there.

## Follow-ups

- **The synth-loop software-pipelining spike** — the one lever that could flip
  the Xtensa default, and the reason the flip condition is written as a
  measurable threshold rather than an intention.
- **The frame-path floor investigation** (≈4,500 cycles/sample at dome scale,
  mode-independent). Its own plan; the other half of the flip condition.
- **Build-time mode sets per image**, per the consequence above — the flash
  the runtime-matched mode currently costs.
- **Gate the pin on the firmware manifest** so it is not offered on a board
  that carries one representation. The compile-error backstop
  (`2026-08-01-float-mode-reaches-the-device.md` §5) stays either way.
- **S31 measurement** when silicon and toolchain arrive: confirm f32-native,
  then take the deferred items above as one milestone.
