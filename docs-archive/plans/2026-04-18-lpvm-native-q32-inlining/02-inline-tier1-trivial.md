# Phase 2 — Inline Tier-1 (Fabs)

## Scope of phase

Convert one Q32 op (`Fabs`) from `sym_call` to an inline VInst sequence.
This phase exists to validate the `TempVRegs` machinery on the simplest
multi-VInst expansion before tackling the multi-temp Tier-2 ops in
Phase 3.

| LPIR op | Inline expansion (Q32 mode) | VInsts | Temps |
| ------- | --------------------------- | ------ | ----- |
| `Fabs`  | `SraiS(31)` + `Xor` + `Sub` | 3      | 2     |

(`Fneg` is already inline as `VInst::Neg` at `lower.rs:478` — no work.)

### Why no other Tier-1 ops?

The original Tier-1 list (`Fadd`, `Fsub`, `ItofS`, `ItofU`) was dropped
after a semantic audit of the helpers. See `00-notes.md` "Q3-revision":

- **`Fadd`/`Fsub`** are i64-widened **saturating** in the helper; naive
  inline `add`/`sub` silently regresses to wrapping. Re-enabled by the
  follow-up plan that wires `Q32Options::add_sub` through `lower_lpir_op`
  and dispatches between sym_call (Saturating) and inline `add`/`sub`
  (Wrapping). Tracked in Phase 4.
- **`ItofS`/`ItofU`** clamp to GLSL-int range before shift; the inlined
  clamp+shift sequence (~5 VInsts) is roughly call cost.

## Code organization reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a `TODO` comment so we can find it later.

## Implementation details

### 2.1 `Fabs` (Q32) — branchless inline matching `wrapping_neg`

Helper semantics (`float_misc_q32.rs:6-8`):

```rust
pub extern "C" fn __lp_lpir_fabs_q32(v: i32) -> i32 {
    if v < 0 { v.wrapping_neg() } else { v }
}
```

Notably: `i32::MIN.wrapping_neg() == i32::MIN`, so a naive bit-mask
inline (`v & 0x7FFFFFFF`) is **wrong** — it would yield `0` for
`i32::MIN`. The branchless idiom that matches `wrapping_neg` exactly is:

```text
mask = v >> 31              ; arithmetic shift: -1 if v<0, 0 otherwise
tmp  = v ^ mask             ; bit-flip if negative; unchanged if non-neg
dst  = tmp - mask           ; +1 if negative (= two's complement neg);
                            ; unchanged if non-neg
```

For `v = i32::MIN` (`0x8000_0000`): `mask = -1`, `tmp = 0x7FFF_FFFF`,
`dst = 0x7FFF_FFFF - (-1) = 0x8000_0000` (overflow wraps back to MIN).
Matches `wrapping_neg` ✓.

Replace the `sym_call` arm at `lower.rs:569`:

```rust
LpirOp::Fabs { dst, src } if float_mode == FloatMode::Q32 => {
    let mask = temps.mint();
    out.push(VInst::AluRRI {
        op: AluImmOp::SraiS,
        dst: mask,
        src: fa_vreg(*src),
        imm: 31,
        src_op: po,
    });
    let tmp = temps.mint();
    out.push(VInst::AluRRR {
        op: AluOp::Xor,
        dst: tmp,
        src1: fa_vreg(*src),
        src2: mask,
        src_op: po,
    });
    out.push(VInst::AluRRR {
        op: AluOp::Sub,
        dst: fa_vreg(*dst),
        src1: tmp,
        src2: mask,
        src_op: po,
    });
    Ok(())
}
```

3 VInsts, 2 temps. First arm to use `temps.mint()` more than once —
proves the watermark increments correctly.

### 2.2 Update unit test

Find the existing test asserting `VInst::Call { target: "__lp_lpir_fabs_q32", … }`
and rewrite to assert the new sequence:

```rust
let v = call_lower_op(&LpirOp::Fabs { dst: …, src: … },
                      FloatMode::Q32, None, &f, &ir, &abi).unwrap();
assert_eq!(v.len(), 3, "Fabs Q32 = SraiS + Xor + Sub");
assert!(matches!(&v[0],
    VInst::AluRRI { op: AluImmOp::SraiS, imm: 31, .. }),
    "first inst should be sra by 31, got: {:?}", v[0]);
assert!(matches!(&v[1], VInst::AluRRR { op: AluOp::Xor, .. }));
assert!(matches!(&v[2], VInst::AluRRR { op: AluOp::Sub, .. }));
```

Avoid pinning temp-VReg ids — they shift if any neighbor's expansion
changes.

Drop the corresponding Phase-1 `assert_eq!(v.len(), 1)` guard rail for
this test (the `// TODO(phase-2)` comment).

## Validate

```
turbo check test
```

Critical signals:

- `Fabs` unit test passes with the new sequence.
- Filetest suite passes — `Fabs` is exercised by GLSL `abs()` calls
  across many `lps-filetests/filetests/` (e.g. `builtins/common-abs.glsl`).
  Particularly important: any test that exercises `abs(MIN)` or
  near-MIN values — these would surface a wrong inline.
- No new warnings.

If a filetest regresses on the abs path, double-check the branchless
sequence: the `SraiS(31)` must be **arithmetic** right shift (sign-fill),
not logical. `AluImmOp::SraiS` is the arithmetic variant; `SrliU` is
logical and would silently break this.
