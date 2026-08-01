# ADR: Float mode is a compiler parameter, not a property of the IR

- **Status:** Accepted
- **Date:** 2026-08-01
- **Deciders:** Photomancer
- **Supersedes:** Partially supersedes
  `2026-07-09-preview-fidelity-tiers.md` — see "What this retires, and what it
  does not" below. Builds on `2026-07-31-xtensa-fp-behavior-contract.md` (the
  measured behaviour of this FPU), `2026-08-01-host-emulator-models-flash.md`
  (why the builtins the float path calls are reachable at all), and
  `docs/design/float.md` (normative float semantics).
- **Superseded by:** None

## Context

LPIR carries `Fadd`, `Fmul`, `Flt` and the rest as *float operations*. It does
not say how a float is represented. `FloatMode` — `Q32` or `F32` — is threaded
through compilation as a **parameter**, and lowering matches on it at runtime.

That single fact drives everything else here, including a consequence that looks
like an unrelated build-system detail: because `FloatMode` is matched on a
runtime value rather than a `cfg`, **LTO cannot drop the arms a given board
never enters.** Dead-code elimination can only remove what the linker can prove
unreachable, and a `match` on a value carried in a struct is not that. This is
measured, not theorised: `isa/xt` cost the ESP32-C6 image +26,448 bytes before a
Cargo feature gated it, for a backend that chip cannot execute a single
instruction of.

Before M7, the only float mode that reached a device was Q32. The ESP32-S3 has a
real FPU (M6-P1's silicon presence table: 16 FRs, `add.s`/`mul.s`/`madd.s`, FSR
flags, no flush-to-zero), so "float on a device" stopped being hypothetical, and
the question of *where in the stack the choice lives* had to be answered.

## Decision

### 1. Float mode is an emitter parameter

One LPIR module compiles to Fixed or Float without re-running the frontend. The
interpreter, wasm, GPU and native tiers already agree on the IR; making float
mode an IR property would fork that agreement and force every consumer to carry
two module shapes.

Concretely: `NativeCompileOptions::float_mode` reaches `compile_module`, which
dispatches to `lower.rs` or `lower_f32.rs`. Nothing above the compiler needs a
second module, and nothing below it needs to ask the frontend anything.

### 2. Hardware-vs-soft float is a property of the *target*, and the result is
### *reported*, not requested

`IsaTarget::f32_lowering()` answers `HardwareFpu`, `SoftFloatCalls`, or
`Unsupported`. rv32 has no FPU and answers `SoftFloatCalls`; Xtensa with
`float-f32` answers `HardwareFpu`.

The asymmetry is the point. "rv32" and "Xtensa" are not two dialects of one
float story — one has no FPU at all and one has a full coprocessor — so the
answer is a named property of the target rather than an assumption baked into
lowering.

Compilation then *discloses* what it actually did:
`FloatImpl::{Fixed, HardwareF32, SoftF32}` is a **result**, read off the
compiled module (roadmap D3). An author asking for Float mode does not get to
specify how; they get told. Before M7 this value was hardcoded to `Fixed`, which
was true and is no longer.

### 3. The feature gate, and what it means for the product

`float-f32` gates *modules* — `lower_f32.rs`, `isa/xt/emit_fp.rs`, and the
Xtensa float register/ABI tables — not enum variants. `VInst`'s float variants
stay unconditional: `cfg` on variants matched exhaustively across five helper
functions and a 1129-line ser/de module costs more than it saves, and the size
check measures the residual rather than anyone guessing at it (D9).

With the feature **off**, `f32_lowering()` answers `Unsupported` and a Float
request reaches the existing catch-all and **errors**. Deliberately not
`SoftFloatCalls`: a board with a real FPU quietly given the slow path would hide
a misconfigured image behind working output. Silence is the failure mode this
avoids — the same principle `2026-07-09-preview-fidelity-tiers.md` §4 states for
GPU tier selection, applied to a different axis.

**Measured, both directions (M7 P5):**

| Image | Value |
|---|---|
| ESP32-C6, before M7's firmware changes | 2,874,560 B |
| ESP32-C6, after (feature never named) | **2,874,560 B — byte-identical** |
| ESP32-S3, `float-f32` **off** | 1,760,656 B |
| ESP32-S3, `float-f32` **on** | 1,826,336 B |
| S3 cost of hardware float | **+65,680 B (+3.7%)** |

The C6's zero delta is the evidence that the gate holds. It is a measurement
taken with the same recipe before and after, not an inspection of what *ought*
to be reachable — which matters, because the +26,448 B that motivated the
mechanism was also invisible to inspection.

### 4. The Xtensa float ABI

**Floats live in FRs inside a function and cross every boundary in address
registers, as raw IEEE bit patterns** (D1/D2) — parameters, call arguments, call
returns, function returns. `wfr` (AR→FR) and `rfr` (FR→AR) are explicit `VInst`s
that *lowering* emits at exactly those points.

This was forced, not chosen. M6-P4 probed the esp toolchain — which compiles
M5's builtins — and measured floats passed in `a2..a7` and returned in `a2`.
That is not negotiable for builtin calls, and a second convention for
LPIR-internal calls would push a mode axis through the sret/vmctx machinery for
no measured gain. Lowering emits the transfers (rather than regalloc or the
emitter) so that the ABI is **visible in the VInst dump** and a filetest can read
it.

**The Float ABI hooks stay empty on purpose** (D3). Because floats never occupy
an ABI argument *slot*, `call_arg_reg(Float, …)`, `direct_ret_reg(Float, …)`,
`direct_ret_reg_count(Float)` and `lpir_call_arg_target(Float, …)` return
`None`/`0` for Xtensa, and `move_cycle_scratch` did not grow the `class`
parameter its M4 doc comment anticipated — float vregs never participate in an
argument-move cycle. Each arm carries a doc comment saying *why it is empty*, so
it reads as decided rather than unimplemented.

**The frame does not change** (D7). No FR is callee-saved (measured, M6-P4), so
there is no FP callee-save region; `FrameLayout::compute` is untouched,
`FRAME_TOP_RESERVED_BYTES` stays 32 and stays FP-free, and the prologue/epilogue
stay one `entry` / one `retw`. Float spills reuse the existing class-tagged
spill space at the frame's **bottom**; the window-overflow handler writes the
reservation at the **top**. They cannot collide.

That argument is worth nothing untested, and its failure mode is the worst kind
— silent corruption of an ancestor frame surfacing long after the return. It is
pinned twice: a depth-100 recursion carrying live floats across every call on
the emulator, with an integer control at the same depth and shape (P4), and the
same shape at depth 20 on **silicon**, past the S3's 16-window ring (P5).

**BR is fused, never allocated** (D5). FP compares write a Boolean register;
`b0` is an implicit scratch and the 0/1 result is materialised into an AR within
the same emitted sequence, so the allocator never learns BRs exist. The
invariant that makes one fixed `b0` safe — **no BR live across a `VInst`
boundary** — is stated in the emitter's module doc.

**15 of the 16 FRs are allocatable, and all are caller-saved.** D8 originally
said all 16; that was wrong and execution found it, not review — a *spilled def*
still needs a register to write to first, so `f15` is reserved emitter scratch.

### 5. What is not inlined

Inlined, one FP instruction each: `fadd` `fsub` `fmul`, `fabs` `fneg` `fmov`,
the six compares and float select, `itof_s` `itof_u`, float load/store, and the
`wfr`/`rfr` transfers.

Routed to an M5 builtin via `sym_call`: `fdiv` `fsqrt`, the rounding family
(`ffloor` `fceil` `ftrunc` `fnearest`), `fmin` `fmax`, the saturating
float→int conversions, the unorm conversions, and every transcendental and
`lpfn` (D4).

This is the same mechanism Q32 already uses, and every symbol exists (M5, PR
#224). A missing symbol **fails loudly at resolution** and never falls through
to a Q32 sibling — a resolver never crosses modes.

The FPU offers `recip0.s`/`rsqrt0.s`/`div0.s` estimate instructions that a
future optimisation could use to inline divide and square root. The sequences
and their exact behaviour are characterised in
`2026-07-31-xtensa-fp-behavior-contract.md`; M7 does not use them, because their
results are implementation-defined and the builtins are already correct.

### 6. Arming the FPU is the host's job, not the compiler's

Compiled float code contains bare FP instructions and arms nothing. On a core
whose `CPENABLE` bit 0 is clear, the first one takes `EXCCAUSE=32`. Enabling a
coprocessor is a property of the **execution context**, which firmware owns, so
`lpvm-native` documents the requirement and does not implement it.
`fw-esp32s3`'s `board::esp32s3::fpu::arm()` does it at board init, and
`lp-xt-emu` leaves `CPENABLE` clear by default so a host that forgets faults on
the desk rather than on a board.

M6-P1 measured this silicon arriving with `CPENABLE == 0xff` under the esp-hal
boot chain, but no `wsr.cpenable` exists in esp-hal 1.1.1 or xtensa-lx-rt 0.22,
so the write's provenance is unpinned. Arming is therefore defensive and is a
**read-modify-write**: a blind store of `1` would disable coprocessors 1–7 on a
board that booted with all of them on.

## What this retires, and what it does not

`2026-07-09-preview-fidelity-tiers.md` is **partially superseded**.

Two of its statements stop being true the moment an ESP32-S3 executes native
f32, which it now does (M7 P5, `passed=27 failed=0` on silicon):

- **"Q32 remains the single authoritative semantics."** There are now two
  authoritative numeric semantics, and which one applies is a property of the
  compiled module, disclosed by `FloatImpl`. `docs/design/float.md` is normative
  for the f32 one, with `docs/design/q32.md` unchanged as normative for Fixed.
- **The scope clause that ESP32 devices keep Q32.** The S3 has an FPU and can
  execute IEEE f32 natively. Q32 remains the *default* everywhere and the only
  option on boards without an FPU — which is still most of them.

What is **not** retired, and is deliberately carried forward unchanged:

- The **preview-fidelity framing itself** — that different tiers may legitimately
  differ in numeric fidelity, and that this is a product decision rather than a
  defect.
- The **GPU tier's documented latitude**, including the measured divergence
  bounds and where divergence concentrates.
- **Tier selection is always explicit and never a silent fallback** (§4 of that
  ADR). M7 extends this principle rather than weakening it: a board without the
  f32 backend linked *errors* on a Float request instead of quietly compiling
  Fixed.
- **Q32-on-GPU is still not built**, and the non-embedded Q32 CPU parity mode is
  untouched.

## Consequences

- **M8** can register `xtn.f32` and `xtlpn.f32` filetest targets: there is now a
  backend for them to point at, and the emulator is the oracle (G2).
- **M9's soft float** reuses this seam unchanged — `FloatImpl::SoftF32` and
  `F32Lowering::SoftFloatCalls` exist because the seam was built to describe a
  target's float capability rather than to describe Xtensa.
- **The product now has two numeric tiers that differ by *board*.** Which boards
  offer Float mode, how a project expresses the choice, and what happens when a
  project targets a board that cannot honour it, are capability-model questions
  this roadmap deliberately deferred. Today the capability is **linked but not
  reachable at runtime** on the S3: `NativeCompileOptions::default()` is Q32 and
  M2's project-level `float_mode` slot is not yet plumbed to the device graphics
  backend. The +65,680 B is currently paid for a path nothing can enter.
- **A new consumer crate must hand-edit `Cargo.toml`** to opt in, in the same
  way the multi-ISA seam requires. `lpvm-native/README.md`'s float-mode seam
  section is the reference.
- **The `float-f32` OFF configuration needs deliberate coverage.** It is in
  `fw-esp32s3`'s `default`, so every ordinary build exercises the on-arms and
  none exercised the off-arms; `just clippy-fw-esp32s3` now carries an explicit
  gate-off pass.

## Alternatives Considered

- **Float mode as an IR property** (two module shapes, chosen in the frontend).
  Rejected: forks the IR agreement every tier depends on, and makes recompiling
  one shader in the other mode a frontend round-trip.
- **`cfg`-gating `VInst`'s float variants** rather than whole modules. Rejected
  on cost: exhaustive matches across five helpers and a 1129-line ser/de module.
  The residual is measured instead of argued (D9).
- **Answering `SoftFloatCalls` on Xtensa when `float-f32` is off.** Rejected:
  it makes a misconfigured image look like a working one, which is the failure
  class this repo consistently refuses.
- **A distinct calling convention for LPIR-internal float calls** (floats in FRs
  across guest→guest boundaries). Rejected: builtin calls must use the esp
  toolchain's AR convention regardless, and two conventions would push a mode
  axis through sret/vmctx for no measured gain.
- **Widening the emulator's SRAM code region** so the f32 builtins fit.
  Rejected, and instructively — see
  `2026-08-01-host-emulator-models-flash.md`. The builtins execute from flash on
  real hardware; widening would have preserved a wrong memory map.

## Follow-ups

- **Plumb M2's `float_mode` slot to the device graphics backend**, so the linked
  capability becomes reachable. Until then the S3 image carries a path it cannot
  enter.
- **The capability model**: which boards offer Float mode, and how a project
  targeting an unsupporting board is handled.
- **M8**: `xtn.f32` / `xtlpn.f32` filetest targets and corpus triage.

## Remains unverified

Stated rather than implied away:

- **Inline divide and square root** via the estimate instructions — not
  attempted; the sequences are characterised but implementation-defined.
- **`trunc.s` saturation** on out-of-range and NaN inputs. M7 routes float→int
  through the builtins, which saturate per `float.md`, so the hardware
  instruction's own behaviour is not on any executed path and was not probed.
- **Contraction.** `madd.s` is fused on this silicon (M6), but M7 emits no
  `madd` — whether the compiler should ever contract `a*b+c` is unanswered, and
  `float.md` classes contraction as Target-defined.
- **The classic ESP32's LX6 FPU.** It exists and is unprobed. The chip's
  viability for f32 is a flash-budget and LX6-probe question, not an SRAM one —
  the earlier "it cannot fit" conclusion was an artifact of the emulator memory
  model corrected in `2026-08-01-host-emulator-models-flash.md`.
- **`divn.s` off the sequence envelope.** Measured 2026-08-01 by M6's second
  probe round: the model reproduces 4 985/6 897 off-envelope probes (72.3%),
  weakest at 41.7% in the class region. Unreachable by emitted code, so it does
  not affect anything M7 ships — but it raises the bar on the inline
  divide/sqrt follow-up above, which would need a re-fit of that model before
  it could be attempted.
- Non-RNE rounding modes beyond add/sub/mul — refused loudly, by decision.
- *(Acc-NaN payload priority was on this list and is now closed — the
  second probe round measured it exactly, 176/176.)*
