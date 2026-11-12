# Phase 4 — Cleanup, doc, validate, commit

## Scope of phase

Final pass: doc the inline-vs-call policy, mark the Q32 helpers as
reference impls, scrub temporary code/TODOs, run full validation, write
the plan summary, move the plan to `docs/plans-done/`, and commit.

## Code organization reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a `TODO` comment so we can find it later.

## Implementation details

### 4.1 Document the inline-vs-call policy in `lower.rs`

Add a module-level doc comment block at the top of
`lp-shader/lpvm-native/src/lower.rs`, immediately after the existing
file-doc:

```rust
//! Q32 op lowering policy
//! ----------------------
//!
//! For each Q32 LPIR op the backend chooses one of four strategies:
//!
//! * **Inline.** Emit a short [`VInst`] sequence that directly performs
//!   the op. Used for ops where the inline expansion matches the helper
//!   bit-for-bit on the i32 input domain and is competitive with the
//!   call cost (typically <= ~6 RV32 instructions).
//!
//!   Currently inlined: `Fneg`, `Fabs`, `Fmin`, `Fmax`, `FtoUnorm16`,
//!   `FtoUnorm8`, `Unorm16toF`, `Unorm8toF`.
//!
//! * **`sym_call` (pending Q32Options dispatch).** Helper performs
//!   saturating arithmetic via i64-widening; naive inline `add`/`sub`
//!   would silently regress to wrapping. Cannot be inlined without a
//!   `Q32Options::add_sub` switch surfaced through the lowering pipeline
//!   to choose the right expansion. Tracked as the immediate follow-up
//!   plan.
//!
//!   Currently call: `Fadd`, `Fsub`.
//!
//! * **`sym_call` (defer for review).** Non-trivial semantics
//!   (saturation, rounding modes, clamping, multi-word arithmetic) that
//!   warrant a dedicated correctness pass before inlining.
//!
//!   Currently call: `Fmul`, `ItofS`, `ItofU`, `FtoiSatS`, `FtoiSatU`,
//!   `Ffloor`, `Fceil`, `Ftrunc`, `Fnearest`. Each is a candidate for a
//!   future inlining plan.
//!
//! * **`sym_call` (permanent).** Operation cost dwarfs the call overhead;
//!   inlining brings no benefit.
//!
//!   Currently call: `Fdiv`, `Fsqrt`.
//!
//! All Q32 helper functions remain in `lps-builtins` as the **reference
//! implementation** of op semantics. Inline expansions must match the
//! helper's behavior bit-for-bit on the i32 input domain.
//!
//! Zbb (`min`/`max`) is not enabled — ESP32-C6 silicon does not decode
//! it. If/when a Zbb-bearing target is added, `Fmin`/`Fmax` can collapse
//! to a single instruction via `AluOp::MinS` / `AluOp::MaxS`.
```

### 4.2 Mark each Q32 helper as reference impl

For each `lp-shader/lps-builtins/src/builtins/lpir/*_q32.rs` file
covering an inlined op, add a one-line module-doc note. Example for
`unorm_conv_q32.rs`:

```rust
//! Q32 unorm channel conversions (`FtoUnorm16` / `FtoUnorm8` / `Unorm16toF` /
//! `Unorm8toF`), matching `lpvm-cranelift` Q32 lowerings.
//!
//! **Reference implementation.** The primary `lpvm-native` lowering inlines
//! these ops directly — see `lpvm-native::lower::lower_lpir_op`. These
//! helpers remain as the authoritative semantic reference and as a fallback
//! safety net.
```

Apply the same one-line note to the other helper files for the inlined
ops only — `float_misc_q32.rs` covers `Fabs`/`Fmin`/`Fmax`. The
add/sub/itof helpers are **not** inlined in this plan and should keep
their existing module docs unchanged.

### 4.3 Scrub temporary code

Grep the full plan diff for leftover scaffolding:

```
git diff main -- lp-shader/lpvm-native/ lp-shader/lps-builtins/ \
  | rg -n 'TODO\(phase|debug_println|dbg!|eprintln|FIXME|XXX|HACK'
```

Expected hits: the `TODO(phase-2)` from Phase 1's `assert_eq!(v.len(), 1)`
guard rails — those should all be replaced by per-test exact length
assertions in Phase 2/3. Anything still flagged is real and must be
removed or justified.

### 4.4 Final validation

```
turbo check test
```

Must pass cleanly with no warnings. If any are introduced (unused
imports from removed `sym_call` arms, dead helpers, etc.), fix them.

Sanity check the perf claim by running one render-heavy filetest under
both old and new lowering — pick `lps-filetests/filetests/debug/rainbow.glsl`
or similar. Compare `--emit asm` output, verify the inlined sites no
longer contain `jal __lp_lpir_*_q32`. (Optional but quick — cite the
inst-count delta for the commit body.)

### 4.5 Plan cleanup

Write `docs/plans/2026-04-18-lpvm-native-q32-inlining/summary.md`:

```md
# Summary — lpvm-native Q32 op inlining

## What changed

- `lpvm-native::lower::lower_lpir_op` switched from
  `Result<VInst, _>` to `Result<(), _>` with sink-param
  `out: &mut Vec<VInst>` and a `TempVRegs` watermark for fresh
  intermediate vregs. Lifts the 1-LPIR-op → 1-VInst constraint.
- Inlined 7 Q32 ops that previously went through `sym_call` runtime
  helpers: `Fabs`, `Fmin`, `Fmax`, `FtoUnorm16`, `FtoUnorm8`,
  `Unorm16toF`, `Unorm8toF`. (`Fneg` was already inline.)
- Q32 helpers in `lps-builtins/src/builtins/lpir/{float_misc,unorm_conv}_q32.rs`
  are kept as the reference implementation; documented as such.
- `lower.rs` module header documents the four-tier inline-vs-call
  policy and per-op rationale.

## What didn't change

- LPIR surface (no new ops, no new `LpirOp` variants).
- `VInst` surface (existing variants suffice).
- Cranelift / WASM / emu backends.
- Op semantics — all expansions match helper bit-for-bit on i32.
- `Fadd`/`Fsub` lowering — still routed through saturating helpers
  (see follow-up).

## Follow-ups

- **Q32Options dispatch (next priority).** Wire
  `Q32Options::add_sub` (already defined in `lps-q32/src/q32_options.rs`)
  through the lowering pipeline so `Fadd`/`Fsub` can pick between
  inline `add`/`sub` (Wrapping) and the saturating helper (Saturating
  default). Ditto `Q32Options::mul` for `Fmul` once the saturating
  expansion lands. Architectural: the option exists from the pre-LPIR
  Cranelift era but no codegen path consumes it today.
- **Inline `Fmul`** — saturating mul is `mul + mulh + reassemble +
  saturate` (~6-8 RV32 insts); wrapping mul is ~3-4. Pair with the
  Q32Options work above.
- **Inline `ItofS`/`ItofU`** clamp+shift expansions — sequence is
  ~5 VInsts, roughly call cost; deprioritized.
- **Inline `FtoiSat[SU]`, `Ffloor`/`Fceil`/`Ftrunc`/`Fnearest`** —
  saturation / rounding-mode review.
- **Zbb-bearing `IsaTarget`** — collapse `Fmin`/`Fmax` to single
  insts when a target supports it (ESP32-C6 does not).
```

Then move the plan directory:

```
mv docs/plans/2026-04-18-lpvm-native-q32-inlining \
   docs/plans-done/2026-04-18-lpvm-native-q32-inlining
```

### 4.6 Commit

```
git add -A
git commit -m "perf(lpvm-native): inline 7 cheap Q32 ops in lower

Lift the 1-LPIR-op → 1-VInst constraint in lpvm-native's lower_lpir_op
by switching to a sink-param API with a TempVRegs watermark. Convert 7
Q32 ops from sym_call runtime helpers to inline VInst sequences:

  Fabs                                             — branchless 3-inst
                                                     (sra31 + xor + sub),
                                                     matches wrapping_neg
  Fmin / Fmax                                      — Icmp + Select
  Unorm16toF / Unorm8toF                           — mask (+ shift)
  FtoUnorm16 / FtoUnorm8                           — clamp + mask

Eliminates the call-site overhead (jal + caller saves) on the pixel
hot path, where every channel of every pixel hits FtoUnorm and texture
samplers hit Unorm*toF. Q32 helpers in lps-builtins remain as the
reference implementation; documented as such.

No semantic change — inline expansions match the Q32 helpers
bit-for-bit on the i32 input domain. Validated by the existing
rv32n.q32 filetest suite (no new test files; ~7 existing lower.rs
unit tests rewritten to assert the new VInst sequences).

Fadd/Fsub deliberately deferred — helpers are i64-saturating; naive
inline would silently regress to wrapping. Re-enabled once
Q32Options::add_sub is plumbed through the lowering pipeline (next
plan).

Plan: docs/plans-done/2026-04-18-lpvm-native-q32-inlining/
"
```

(Adjust commit-body details once the actual diff is in hand.)

## Validate

Final run after commit:

```
turbo check test
git status                # should be clean
git log -1 --stat         # commit looks right
```
