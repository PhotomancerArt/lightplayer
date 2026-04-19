# lpvm-native Q32 op inlining — Design

## Scope of work

`lpvm-native` (RV32 backend) currently lowers every LPIR op to **exactly
one** `VInst`. For Q32 mode all float ops bottom out in `sym_call` → a
`VInst::Call` to a runtime helper in `lps-builtins`. For trivial ops
(`Fadd` = `add`, `Fmin` = `Icmp+Select`, `FtoUnorm16` = clamp+mask) this is
real perf overhead on the pixel hot path: full call-site cost (jal,
prologue, epilogue, caller-saves) for what should be 1–6 instructions
inline.

This plan:

1. **Lifts the 1-LPIR-op → 1-VInst constraint** in
   `lpvm-native::lower::lower_lpir_op` by switching it to a sink-parameter
   API (`out: &mut Vec<VInst>`) plus a `TempVRegs` watermark for fresh
   intermediate vregs.
2. **Inlines a curated set of 7 cheap Q32 ops** that match their helpers
   bit-for-bit on the i32 input domain (Tier 1: `Fabs`; Tier 2: `Fmin`,
   `Fmax`, `FtoUnorm16`, `FtoUnorm8`, `Unorm16toF`, `Unorm8toF`). `Fneg`
   is already inline.

Out of scope:

- **`Fadd` / `Fsub`** — helpers are i64-widened **saturating**; naive
  `add`/`sub` would silently regress to wrapping. Requires plumbing
  `Q32Options::add_sub` through the lowering pipeline (the option exists
  in `lps-q32/src/q32_options.rs` but no codegen path consumes it yet —
  legacy from pre-LPIR Cranelift architecture). Sole motivation for the
  immediate **follow-up plan** referenced in Phase 4.
- **`ItofS` / `ItofU`** — helpers clamp to GLSL-int range before shift;
  inlined sequence (~5 VInsts) is roughly call-cost; no real win.
- **`Fmul`** — saturating Q32 mul = `mul + mulh` then 64→32 reassemble +
  saturate, ~6-8 RV32 insts. Doable, but warrants its own correctness
  review (saturation, zero shortcut, sign handling).
- **`FtoiSat[SU]`, `Ffloor`/`Fceil`/`Ftrunc`/`Fnearest`** — saturation /
  rounding-mode semantics review.
- **`Fdiv`, `Fsqrt`** — operation cost dwarfs call overhead.
- Zbb-bearing `IsaTarget` (native `min`/`max`) — ESP32-C6 doesn't decode
  Zbb.
- Pruning Q32 helper functions in `lps-builtins` — kept as reference.
- Inlining in `lpvm-cranelift` or `lpvm-wasm` — both already inline.

## File structure

```
docs/plans/2026-04-18-lpvm-native-q32-inlining/
├── 00-notes.md                       # NEW: scope, current state, Q&A
├── 00-design.md                      # NEW: this file
├── 01-lower-api-refactor.md          # NEW: phase 1 — sink-param + TempVRegs
├── 02-inline-tier1-trivial.md        # NEW: phase 2 — Fadd/Fsub/Fabs/Itof*
├── 03-inline-tier2-hot-path.md       # NEW: phase 3 — Fmin/Fmax/FtoUnorm/Unorm*toF
└── 04-cleanup-and-validate.md        # NEW: phase 4 — docs, validation, commit

lp-shader/lpvm-native/src/
├── lower.rs                          # UPDATE: API refactor + inlining
├── vinst.rs                          # (no change — VInst variants suffice)
└── (no other source files touched)

lp-shader/lps-builtins/src/builtins/lpir/
├── add_q32.rs                        # UPDATE: doc comment marking as ref impl
├── unorm_conv_q32.rs                 # UPDATE: same
├── (others...)                       # UPDATE: same
```

## Conceptual architecture

### Before (current)

```
lower_ops loop:
  for each LpirOp in body:
    self.out.push(lower_lpir_op(op)?);   // exactly 1 VInst per op
                            │
                            ├── Fadd (Q32) → VInst::Call("__lp_lpir_fadd_q32")
                            ├── Fmin (Q32) → VInst::Call("__lp_lpir_fmin_q32")
                            └── ...

emit:
  VInst::Call → jal + caller-saves + ret
```

### After

```
lower_ops loop:
  let mut temps = TempVRegs::new(func.vreg_types.len() as u16);
  for each LpirOp in body:
    lower_lpir_op(&mut self.out, op, ..., &mut temps)?;  // 1..N VInsts
                            │
                            ├── Fabs (Q32) → SraiS(31) + Xor + Sub                (3)
                            ├── Fmin (Q32) → Icmp(LtS) + Select                   (2)
                            ├── FtoUnorm16 → IConst32(0) + Icmp + Select +
                            │                IConst32(65535) + Icmp + Select       (6)
                            ├── Fadd (Q32) → VInst::Call("__lp_lpir_fadd_q32")     (1)
                            │                 [pending Q32Options dispatch]
                            └── Fdiv (Q32) → VInst::Call("__lp_lpir_fdiv_q32")     (1)
                                              [stays — never inline]

emit:
  AluRRR/AluRRI/IConst32/Icmp/Select → straight RV32 sequences (no jal)
```

### Components

- **`lower_lpir_op` (refactored).** Sink-param API:
  `pub fn lower_lpir_op(out: &mut Vec<VInst>, op, …, temps: &mut TempVRegs) -> Result<(), LowerError>`.
  Each match arm pushes 1..N `VInst`s into `out`. Existing arms (Iadd,
  Icmp, Br, etc.) are mechanically rewritten from `Ok(VInst::Foo {…})` to
  `out.push(VInst::Foo {…}); Ok(())`. New inlined arms emit multi-VInst
  expansions.

- **`TempVRegs` (new).** Tiny newtype wrapping `u16`. Owned by the
  per-function lowering state in `lower_ops`, initialized from
  `func.vreg_types.len()`, monotonic. `mint()` returns a fresh `VReg`.

- **Lowering policy (documented in `lower.rs` module header).** Four
  tiers: **inline always** (`Fneg`, `Fabs`, `Fmin`, `Fmax`,
  `FtoUnorm{16,8}`, `Unorm{16,8}toF`); **stay-call pending Q32Options**
  (`Fadd`, `Fsub` — silently regresses correctness without the
  saturating-vs-wrapping dispatch); **inline pending review** (`Fmul`,
  `Itof[SU]` clamp shape, `FtoiSat[SU]`, `Ffloor`/`Fceil`/`Ftrunc`/
  `Fnearest`); **never inline** (`Fdiv`, `Fsqrt`).

- **Reference impls (untouched semantics).** Q32 helper functions in
  `lps-builtins/src/builtins/lpir/*_q32.rs` stay as the source of truth
  for op semantics. Each gets a brief doc comment noting they are the
  reference implementation; primary `lpvm-native` lowering inlines.

### Test strategy

Lean on existing `rv32n.q32` filetests for correctness — every inlined op
is exercised dozens of times across `lps-filetests/filetests/`. Update the
~7 existing `lower.rs` unit tests that assert against
`VInst::Call { target: "__lp_lpir_*_q32", … }` for the inlined ops to
assert the new VInst sequences (these tests need to change anyway and
double as inline-spec documentation). Tests for ops staying on `sym_call`
(`Fadd`/`Fsub`/`ItofS`/`ItofU`/etc.) are untouched.  No new test files.

## Validation

End of every phase: `turbo check test`. Phase 1 must pass with zero
behavioral change. Phases 2–3 must pass full `rv32n.q32` filetest suite —
correctness regressions in the inlined ops will surface there.
