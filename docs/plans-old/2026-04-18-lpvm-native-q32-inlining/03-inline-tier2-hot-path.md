# Phase 3 — Inline Tier-2 (hot-path Q32 ops)

## Scope of phase

Convert 6 Q32 ops from `sym_call` to inline VInst sequences. These are
the ops that hit the `__render_texture` pixel hot path (every channel of
every pixel). All require multi-VInst expansions — this phase exercises
the `TempVRegs` machinery in earnest.

| LPIR op       | Inline expansion (Q32 mode)                                                                          | VInsts | Temps |
| ------------- | ---------------------------------------------------------------------------------------------------- | ------ | ----- |
| `Fmin`        | `Icmp(LtS)` + `Select`                                                                                | 2      | 1     |
| `Fmax`        | `Icmp(GtS)` + `Select`                                                                                | 2      | 1     |
| `FtoUnorm16`  | `IConst32(0)` + `Icmp(LtS)` + `Select` + `IConst32(65535)` + `Icmp(GtS)` + `Select`                   | 6      | 4     |
| `FtoUnorm8`   | `AluRRI::SraiS(8)` + `IConst32(0)` + `Icmp(LtS)` + `Select` + `IConst32(255)` + `Icmp(GtS)` + `Select`| 7      | 5     |
| `Unorm16toF`  | `IConst32(0xFFFF)` + `AluRRR::And`                                                                    | 2      | 1     |
| `Unorm8toF`   | `AluRRI::Andi(0xFF)` + `AluRRI::Slli(8)`                                                              | 2      | 1     |

Reference semantics in `lp-shader/lps-builtins/src/builtins/lpir/unorm_conv_q32.rs`:

- `FtoUnorm16(v)` = `v.max(0).min(65535)`
- `FtoUnorm8(v)`  = `(v >> 8).max(0).min(255)`
- `Unorm16toF(v)` = `v & 0xFFFF`
- `Unorm8toF(v)`  = `(v & 0xFF) << 8`

## Code organization reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a `TODO` comment so we can find it later.

## Implementation details

### 3.1 `Fmin`, `Fmax` (Q32)

Replace the `sym_call` arms at `lower.rs:577` and `lower.rs:585`. Both
are 2-VInst expansions: an integer compare (Q32 ordering matches signed
i32 ordering) followed by a `Select`.

```rust
LpirOp::Fmin { dst, lhs, rhs } if float_mode == FloatMode::Q32 => {
    let cmp = temps.mint();
    out.push(VInst::Icmp {
        dst: cmp,
        lhs: fa_vreg(*lhs),
        rhs: fa_vreg(*rhs),
        cond: IcmpCond::LtS,
        src_op: po,
    });
    out.push(VInst::Select {
        dst: fa_vreg(*dst),
        cond: cmp,
        if_true: fa_vreg(*lhs),
        if_false: fa_vreg(*rhs),
        src_op: po,
    });
    Ok(())
}
LpirOp::Fmax { dst, lhs, rhs } if float_mode == FloatMode::Q32 => {
    let cmp = temps.mint();
    out.push(VInst::Icmp {
        dst: cmp,
        lhs: fa_vreg(*lhs),
        rhs: fa_vreg(*rhs),
        cond: IcmpCond::GtS,
        src_op: po,
    });
    out.push(VInst::Select {
        dst: fa_vreg(*dst),
        cond: cmp,
        if_true: fa_vreg(*lhs),
        if_false: fa_vreg(*rhs),
        src_op: po,
    });
    Ok(())
}
```

NaN handling: Q32 has no NaN representation, so this matches the
helper's `if a < b { a } else { b }` semantics exactly.

### 3.2 `Unorm16toF`, `Unorm8toF` (Q32)

Replace arms at `lower.rs:654` and `lower.rs:662`. `0xFFFF` doesn't fit
in a 12-bit signed immediate (Andi sign-extends), so `Unorm16toF` needs
`IConst32` + `AluRRR::And`. `0xFF` fits — `Unorm8toF` can use `Andi`.

```rust
LpirOp::Unorm16toF { dst, src } if float_mode == FloatMode::Q32 => {
    let mask = temps.mint();
    out.push(VInst::IConst32 {
        dst: mask,
        val: 0xFFFF,
        src_op: po,
    });
    out.push(VInst::AluRRR {
        op: AluOp::And,
        dst: fa_vreg(*dst),
        src1: fa_vreg(*src),
        src2: mask,
        src_op: po,
    });
    Ok(())
}
LpirOp::Unorm8toF { dst, src } if float_mode == FloatMode::Q32 => {
    let masked = temps.mint();
    out.push(VInst::AluRRI {
        op: AluImmOp::Andi,
        dst: masked,
        src: fa_vreg(*src),
        imm: 0xFF,
        src_op: po,
    });
    out.push(VInst::AluRRI {
        op: AluImmOp::Slli,
        dst: fa_vreg(*dst),
        src: masked,
        imm: 8,
        src_op: po,
    });
    Ok(())
}
```

### 3.3 `FtoUnorm16` (Q32)

Replace arm at `lower.rs:638`. Implements `v.max(0).min(65535)`. `0` and
`65535` both need `IConst32` (65535 doesn't fit signed-12-bit). Two
clamp stages, each a compare-and-select.

```rust
LpirOp::FtoUnorm16 { dst, src } if float_mode == FloatMode::Q32 => {
    let zero = temps.mint();
    out.push(VInst::IConst32 { dst: zero, val: 0, src_op: po });

    // step 1: lo = max(v, 0) ⇒ if v < 0 then 0 else v
    let cmp_lo = temps.mint();
    out.push(VInst::Icmp {
        dst: cmp_lo, lhs: fa_vreg(*src), rhs: zero,
        cond: IcmpCond::LtS, src_op: po,
    });
    let lo = temps.mint();
    out.push(VInst::Select {
        dst: lo, cond: cmp_lo,
        if_true: zero, if_false: fa_vreg(*src),
        src_op: po,
    });

    // step 2: hi = min(lo, 65535) ⇒ if lo > 65535 then 65535 else lo
    let cap = temps.mint();
    out.push(VInst::IConst32 { dst: cap, val: 65535, src_op: po });
    let cmp_hi = temps.mint();
    out.push(VInst::Icmp {
        dst: cmp_hi, lhs: lo, rhs: cap,
        cond: IcmpCond::GtS, src_op: po,
    });
    out.push(VInst::Select {
        dst: fa_vreg(*dst), cond: cmp_hi,
        if_true: cap, if_false: lo,
        src_op: po,
    });
    Ok(())
}
```

6 VInsts, 4 temps. Conservative — could be tightened later (e.g. fold
the `IConst32(0)` into reusing `x0`/zero register if the emitter lets
us, or use `IcmpImm` against imm=0 for the low-side compare). Defer
those optimizations; correctness first.

### 3.4 `FtoUnorm8` (Q32)

Replace arm at `lower.rs:646`. Same shape as `FtoUnorm16` but with a
leading `SraiS(8)` (matches helper's `v >> 8` arithmetic shift), and the
high cap is 255 — fits in 12-bit signed imm, so the second `IConst32`
could become `IcmpImm`. Keep the structure parallel to `FtoUnorm16` for
readability:

```rust
LpirOp::FtoUnorm8 { dst, src } if float_mode == FloatMode::Q32 => {
    // step 0: shift Q32 value right by 8 to get unorm8-scale value
    let shifted = temps.mint();
    out.push(VInst::AluRRI {
        op: AluImmOp::SraiS,
        dst: shifted,
        src: fa_vreg(*src),
        imm: 8,
        src_op: po,
    });

    let zero = temps.mint();
    out.push(VInst::IConst32 { dst: zero, val: 0, src_op: po });

    let cmp_lo = temps.mint();
    out.push(VInst::Icmp {
        dst: cmp_lo, lhs: shifted, rhs: zero,
        cond: IcmpCond::LtS, src_op: po,
    });
    let lo = temps.mint();
    out.push(VInst::Select {
        dst: lo, cond: cmp_lo,
        if_true: zero, if_false: shifted,
        src_op: po,
    });

    let cap = temps.mint();
    out.push(VInst::IConst32 { dst: cap, val: 255, src_op: po });
    let cmp_hi = temps.mint();
    out.push(VInst::Icmp {
        dst: cmp_hi, lhs: lo, rhs: cap,
        cond: IcmpCond::GtS, src_op: po,
    });
    out.push(VInst::Select {
        dst: fa_vreg(*dst), cond: cmp_hi,
        if_true: cap, if_false: lo,
        src_op: po,
    });
    Ok(())
}
```

7 VInsts, 5 temps.

### 3.5 Update unit tests

Same pattern as Phase 2. For multi-VInst expansions, assert structure
with `matches!`:

```rust
let v = call_lower_op(&LpirOp::Fmin { … }, FloatMode::Q32, …).unwrap();
assert_eq!(v.len(), 2, "Fmin Q32 = Icmp + Select");
assert!(matches!(&v[0], VInst::Icmp { cond: IcmpCond::LtS, .. }));
assert!(matches!(&v[1], VInst::Select { .. }));

let v = call_lower_op(&LpirOp::FtoUnorm16 { … }, FloatMode::Q32, …).unwrap();
assert_eq!(v.len(), 6, "FtoUnorm16 Q32 = IConst32+Icmp+Select+IConst32+Icmp+Select");
assert!(matches!(&v[0], VInst::IConst32 { val: 0, .. }));
assert!(matches!(&v[1], VInst::Icmp  { cond: IcmpCond::LtS, .. }));
assert!(matches!(&v[2], VInst::Select { .. }));
assert!(matches!(&v[3], VInst::IConst32 { val: 65535, .. }));
assert!(matches!(&v[4], VInst::Icmp  { cond: IcmpCond::GtS, .. }));
assert!(matches!(&v[5], VInst::Select { .. }));
```

Don't pin temp-VReg ids — they'll shift if any neighboring op's
expansion changes.

## Validate

```
turbo check test
```

Critical signals:

- All `lpvm-native` unit tests pass.
- Full `rv32n.q32` filetest suite passes — particularly the
  `__render_texture`-using shader tests (see `lps-filetests/filetests/`
  rainbow / debug tests). Channel-output correctness requires
  `FtoUnorm16` / `Unorm16toF` to round-trip identically to the helper.
- `Fmin`/`Fmax` are exercised by `vec/vec4/fn-max.gen.glsl`,
  `builtins/common-clamp.glsl`, and many `builtins/*` tests.
- No `VInst::Call { target: "__lp_lpir_{fadd,fsub,fabs,fmin,fmax,itof_*,fto_unorm*,unorm*_to_f}_q32", .. }`
  remains in the compiled output for Q32 mode (regression check —
  visually inspect a `--emit asm` output of one filetest if uncertain).

If a filetest regresses, diff inline expansion against the helper in
`lps-builtins/src/builtins/lpir/*_q32.rs` — they're the reference
implementation and must match bit-for-bit on the i32 input domain.
