# lpvm-native: Q32 op inlining

## Scope of work

`lpvm-native` (RV32 backend) currently lowers every LPIR op to **exactly one**
`VInst`. For Q32 mode all float ops (Fadd, Fmin, FtoUnorm16, …) bottom out in
`sym_call` → a `VInst::Call` to a runtime helper in `lps-builtins`. For the
trivial ones (Fadd = iadd, Fmin = Icmp+Select, FtoUnorm16 = clamp + mask) this
is a real perf hit on the pixel hot path: full call-site cost (jal, prologue,
epilogue, caller-saves) for what should be 1–6 instructions inline.

This plan does two things:

1. **API refactor.** Lift the 1-LPIR-op → 1-VInst constraint in
   `lpvm-native::lower::lower_lpir_op`. After this phase, lowering an op may
   emit multiple `VInst`s.
2. **Inline a curated set of cheap Q32 ops.** Replace `sym_call(...)` with an
   explicit `VInst` sequence for the ops where it pays off. Helper functions in
   `lps-builtins` stay (they're not called → not linked), so this is purely a
   codegen change for `lpvm-native`.

Out of scope:

- Inlining ops in `lpvm-cranelift` or `lpvm-wasm` (already inlined where it
  matters — both backends lower 1:N as a matter of course).
- Pruning unused Q32 helpers from `lps-builtins`. Static registry; not called
  → not linked into the shader binary today.
- Changing IR-level inlining (the parked `feature/inline` branch) or the ISA
  decoupling refactor (`docs/plans/2026-04-17-lpvm-native-isa-decoupling/`).

## Current state of the code

### Lowering shape

- `lower_lpir_op` (`lp-shader/lpvm-native/src/lower.rs:62`) returns
  `Result<VInst, LowerError>`.
- Sole production caller: `lower_ops` at `lower.rs:1190`:

```rust
self.out.push(lower_lpir_op(other, …)?);
```

i.e. one push per LPIR op. The other `self.out.push(VInst::…)` sites in the
same function (control flow, branches, labels) already emit multiple VInsts
per LPIR op directly — so the engine _is_ multi-VInst-aware; only the
per-op helper path is restricted.

- Test helpers `call_lower_op` / `call_lower_op_full` (`lower.rs:1401-1426`)
  also assume a single `VInst` result; they'll need to surface a slice/vec.

### Q32 sym_call sites

All Q32 float ops in `lower.rs` go through `sym_call` → `VInst::Call` to a
runtime helper:

| LPIR op (Q32 mode)                         | Helper symbol                               | Inline cost (RV32 insts)                   |
| ------------------------------------------ | ------------------------------------------- | ------------------------------------------ |
| `Fadd`                                     | `__lp_lpir_fadd_q32`                        | 1 (`add`)                                  |
| `Fsub`                                     | `__lp_lpir_fsub_q32`                        | 1 (`sub`)                                  |
| `Fneg`                                     | (none — already handled?)                   | 1 (`neg` pseudo)                           |
| `Fmul`                                     | `__lp_lpir_fmul_q32`                        | ~3 (`mul`, `mulh`, shift+or)               |
| `Fdiv`                                     | `__lp_lpir_fdiv_q32`                        | many — keep                                |
| `Fsqrt`                                    | `__lp_lpir_fsqrt_q32`                       | many — keep                                |
| `Fabs`                                     | `__lp_lpir_fabs_q32`                        | 2 (`lui`+`and` or `IConst32`+`and`)        |
| `Fmin` / `Fmax`                            | `__lp_lpir_fmin_q32` / `_fmax_q32`          | 2 (`Icmp`+`Select`)                        |
| `Ffloor` / `Fceil` / `Ftrunc` / `Fnearest` | `__lp_lpir_f{floor,ceil,trunc,nearest}_q32` | 5–10 — defer                               |
| `FtoiSatS` / `FtoiSatU`                    | `__lp_lpir_ftoi_sat_{s,u}_q32`              | 4–6 — defer or include                     |
| `ItofS` / `ItofU`                          | `__lp_lpir_itof_{s,u}_q32`                  | 1 (left shift by 16)                       |
| `FtoUnorm16` / `FtoUnorm8`                 | `__lp_lpir_fto_unorm{16,8}_q32`             | ~4–6 (clamp + mask)                        |
| `Unorm16toF` / `Unorm8toF`                 | `__lp_lpir_unorm{16,8}_to_f_q32`            | 1 (already low N bits = unorm; just `Mov`) |

### VInst building blocks (already exist, no new variants needed for v1)

`AluRRR`, `AluRRI`, `IConst32`, `Icmp`, `Select`, `Mov`, `Neg`, `Bnot`,
`Load*`/`Store*`. Notably **no min/max instruction** in base RV32 — Fmin/Fmax
expand to Icmp+Select, just like the helper does today.

### VReg allocation for temps

`VReg(u16)` is just an opaque id. There is no central allocator: regalloc
derives `max_vreg` from the VInst stream + pool (`regalloc/walk.rs:74,136`).
So inlining can mint fresh ids `>= func.vreg_types.len()`. We need to thread
a `&mut u16` (or equivalent) watermark through `lower_lpir_op` so the
expansions don't collide with each other or with IR vregs.

### Other constraints

- `src_op: u16` field on every VInst already supports many VInsts mapping back
  to the same LPIR op index — used by debug/region tooling. No change needed.
- Concurrent plan `2026-04-17-lpvm-native-isa-decoupling/` is purely about
  removing `crate::isa::rv32::*` references from orchestration — orthogonal,
  no merge conflict expected with the lowering signature change.

## Open questions

(Asked one at a time in chat, answers recorded below.)

- **Q1.** ✅ **Decided: A (sink param).** New signature:

  ```rust
  pub fn lower_lpir_op(
      out: &mut Vec<VInst>,
      op: &LpirOp,
      float_mode: FloatMode,
      src_op: Option<u32>,
      func: &IrFunction,
      ir: &LpirModule,
      abi: &ModuleAbi,
      symbols: &mut ModuleSymbols,
      vreg_pool: &mut Vec<VReg>,
      temps: &mut TempVRegs, // see Q2
  ) -> Result<(), LowerError>;
  ```

  Mirrors the existing `vreg_pool: &mut Vec<VReg>` convention; the only
  production caller (`lower_ops`) passes `&mut self.out` directly — zero
  per-call allocation in either the 1:1 or 1:N case. Test helpers
  `call_lower_op` / `call_lower_op_full` (`lower.rs:1401-1426`) become
  return-`Vec<VInst>` for ergonomics.

- **Q2.** ✅ **Decided: monotonic watermark + `TempVRegs` newtype, owned by the
  per-function lowering state.**

  ```rust
  pub struct TempVRegs(u16);
  impl TempVRegs {
      pub fn new(after_ir: u16) -> Self { Self(after_ir) }
      pub fn mint(&mut self) -> VReg {
          let v = VReg(self.0);
          self.0 = self.0.checked_add(1).expect("vreg space exhausted");
          v
      }
  }
  ```

  Initialized once at `lower_ops` entry to `func.vreg_types.len() as u16`.
  Threaded into `lower_lpir_op` as `&mut TempVRegs`. Never resets between
  ops — keeps regalloc's reasoning simple and avoids a footgun if a future
  expansion ever needs a temp to live across boundaries. `max_vreg`
  overhead is bounded by `(# inlined sites) × (max temps per op)`,
  negligible vs the existing `+ 32`/`+ 16` slack in
  `regalloc/walk.rs:142,151`.

- **Q3.** ✅ **Decided: v1 inlines 7 Q32 ops** (revised after semantic
  audit of helpers; see `Q3-revision` note below).

  **Inline (v1) — all match helper bit-for-bit on i32 input:**
  - `Fabs` — 3 insts branchless: `mask = SraiS(v, 31); tmp = Xor(v, mask);
    dst = Sub(tmp, mask)`. Matches helper's `wrapping_neg` exactly,
    including `i32::MIN.abs() == i32::MIN`.
  - `Fmin` — 2 VInsts (`Icmp(LtS)` + `Select`); ~3-4 RV32 after Select
    expansion.
  - `Fmax` — 2 VInsts (`Icmp(GtS)` + `Select`).
  - `FtoUnorm16` — clamp to [0, 65535]: `IConst32(0)` + `Icmp(LtS)` +
    `Select` + `IConst32(65535)` + `Icmp(GtS)` + `Select`. 6 VInsts.
  - `FtoUnorm8` — `SraiS(8)` + same clamp shape capped at 255. 7 VInsts.
  - `Unorm16toF` — `IConst32(0xFFFF)` + `AluRRR::And`. 2 VInsts.
  - `Unorm8toF` — `AluRRI::Andi(0xFF)` + `AluRRI::Slli(8)`. 2 VInsts.

  (`Fneg` is already inline as `VInst::Neg` — no work.)

  **Stay sym_call (this plan; addressed by follow-up `B`):**
  - `Fadd`, `Fsub` — helpers are i64-widened **saturating**; naive `add`/
    `sub` would silently regress to wrapping. Requires `Q32Options::add_sub`
    dispatch (see Q3-revision and Phase 4 follow-up note).

  **Stay sym_call (this plan; deferred indefinitely):**
  - `ItofS` / `ItofU` — helpers clamp to GLSL-int range
    (`[-32768, 32767]` / `[0, 32767]`) before `<< 16`. Inlined sequence is
    ~5 VInsts ≈ helper call cost; not worth the inlining noise.

  **Defer to dedicated follow-up plan(s) (need own correctness reviews):**
  - `Fmul` — Q32 mul = `mul` + `mulh` for 64-bit product, shift/reassemble
    by 16, then saturate. ~6-8 RV32 insts saturating, ~3-4 wrapping.
  - `FtoiSatS` / `FtoiSatU` — saturation semantics review.
  - `Ffloor` / `Fceil` / `Ftrunc` / `Fnearest` — rounding-mode review.

  **Stay sym_call permanently:**
  - `Fdiv`, `Fsqrt` — operation cost dwarfs call overhead.

### Q3-revision: helper semantic audit

After the original Q3 list was drafted, audit of the actual `__lp_lpir_*_q32`
helpers turned up several mismatches between "obvious inline" and helper
semantics:

| Op | Helper behavior | Naive inline | Fix |
| -- | --------------- | ------------ | --- |
| `Fadd`/`Fsub` | i64 widen → saturate to `[i32::MIN, 0x7FFFFFFF]` | `add`/`sub` (wrapping) | drop from v1; pending Q32Options dispatch |
| `Fabs` | `if v<0 { wrapping_neg(v) } else { v }` (so `MIN.abs() == MIN`) | `IConst32(0x7FFFFFFF)` + `And` (gives `MIN.abs() == 0`) | branchless `sra31 + xor + sub` (matches `wrapping_neg`) |
| `ItofS` | clamp `[-32768, 32767]` then `<< 16` | `Slli(16)` only | drop from v1 (clamp adds ~4 insts; ≈ call cost) |
| `ItofU` | clamp `[0, 32767]` (negatives → 32767) then `<< 16` | `Slli(16)` only | drop from v1 (same reason) |

Architectural context: `Q32Options { add_sub: AddSubMode, mul: MulMode, … }`
exists in `lps-q32/src/q32_options.rs` with `Saturating` defaults, but **no
codegen path consumes it** today — the previous architecture (custom GLSL
frontend → Cranelift) had fast-math options; the current architecture (naga
→ LPIR → 3 backends) opted for correctness-by-default and never re-wired
the option through. Wiring it through `lower_lpir_op` is the natural
follow-up plan; out of scope here to keep this commit focused on the API
refactor + the safe inlines.

- **Q4.** ✅ **Decided: keep all `__lp_lpir_*_q32` helpers in `lps-builtins`.**

  They are the **reference implementation** for Q32 op semantics —
  cross-referenced by lpvm-emu, used as oracles during testing, and the
  fallback path if any inlining is reverted. Static-registry; not called →
  not linked into shader binaries. Zero binary cost.

  Add a brief module-level doc comment to each `*_q32.rs` helper file noting
  "reference implementation; the primary `lpvm-native` lowering inlines
  the op directly — see `lpvm-native::lower::lower_lpir_op`."

- **Q5.** ✅ **Decided: lean on existing `rv32n.q32` filetests for
  correctness; targeted unit tests only where strictly needed.**

  Reasoning: every inlined op (`Fadd`/`Fsub`/`Fneg`/`Fabs`/`Fmin`/`Fmax`/
  `Itof*`/`FtoUnorm*`/`Unorm*toF`) is exercised by hundreds of existing
  `rv32n.q32` filetests across `lps-filetests/filetests/` (scalar arithmetic,
  vec ops, builtins, render*texture). Any incorrect lowering will show up
  as a filetest diff — these are \_fundamental* ops; correctness regressions
  are immediately visible.

  **Concrete test plan:**
  1. **Update existing `lower.rs` unit tests** that assert against
     `VInst::Call { target: "__lp_lpir_*_q32", … }` for the inlined ops.
     They have to change anyway (the call no longer exists). Rewrite them
     to assert the new VInst sequence — these double as documentation of
     what each op lowers to.
  2. **Skip new snapshot tests / `debug_asm.rs` additions** — too brittle
     vs. the value they add over filetests.
  3. **Skip the regression guard** — filetest perf on rv32n.q32 will reveal
     accidental regressions to `sym_call` at the function-call cost.
  4. **Skip a microbenchmark** — bench infra not set up; defer to when
     fw-emu integration is in.

  Net new test surface: zero new test files; ~12 existing tests updated.

- **Q6.** ✅ **Decided: 4 phase files, single commit at the end.**
  - **Phase 1 — API refactor.** Sink-param + `TempVRegs`. No behavior
    change; all ops still emit 1 VInst. Tests pass identically.
  - **Phase 2 — Inline Tier-1 (trivial).** `Fadd`, `Fsub`, `Fabs`, `ItofS`,
    `ItofU` (1–2 insts each, no temps). `Fneg` already inline.
  - **Phase 3 — Inline Tier-2 (hot path).** `Fmin`, `Fmax`, `FtoUnorm16`,
    `FtoUnorm8`, `Unorm16toF`, `Unorm8toF` (multi-VInst, exercises
    `TempVRegs`). The actual perf motivation.
  - **Phase 4 — Cleanup, doc, validate, summary, single commit.**

## Notes

### Zbb (min/max) — explicitly out of scope

`min`/`max`/`minu`/`maxu` are Zbb instructions, **not** part of base RV32I —
confirmed against `oss/riscv-isadoc/source/rvb.adoc` (RVB section, MINMAX/CLMUL
funct7 = 0x05) and our own emulator's `// Zbb: Min/Max` labeling at
`lp-riscv-emu/src/emu/executor/arithmetic.rs:54-58`.

ESP32-C6 silicon (the only production target today) is `rv32imac` and does
**not** decode Zbb. Our emulator is permissive and decodes Zbb on top of
`imac`, but emitting native `min`/`max` would trap as illegal-instruction on
real hardware.

So this plan emits `Icmp + Select` for `Fmin`/`Fmax`. If/when we add a
Zbb-bearing target (future Espressif SoC, host-execution backend with FP
min/max, etc.), introduce `AluOp::MinS`/`MaxS`/`MinU`/`MaxU` and route by
`IsaTarget`. Tracked here, follow-up plan when needed.

### Why `Fmin`/`Fmax` expand to `Icmp + Select` (no native min/max on rv32imac)

`min`/`minu`/`max`/`maxu` live in the **Zbb** (Basic Bit-Manipulation)
extension, not in `I`/`M`/`A`/`C`. ESP32-C6 — our only production target —
is plain `rv32imac` and does **not** include Zbb. (The `A` extension has
`amomin.w`/`amomax.w` for atomic memory min/max, and `F`/`D` have
`fmin.s`/`fmax.s` — none of which we have.)

`AluOp` (`vinst.rs:104`) consequently has no `Min`/`Max` variants. Q32
`Fmin`/`Fmax` lower to `Icmp(LtS) + Select` ≈ 3–4 RV32 insts after Select
expansion. That's the shortest sequence available on this target and
matches what `__lp_lpir_fmin_q32` does internally today (just inlined).

If we ever add a Zbb-bearing target (or a host-execution backend with FP
min/max), add `AluOp::MinS`/`MaxS`/`MinU`/`MaxU` and route by
`IsaTarget`. Out of scope for this plan.
