# Phase 3 — `lpvm-native` dispatch

## Scope of phase

Implement Q32Options-driven dispatch for `Fadd`, `Fsub`, `Fmul`, `Fdiv` in
`lpvm-native`'s `lower_lpir_op`:

- `Q32Options::add_sub == Wrapping`: emit inline `AluRRR { Add | Sub }` (1
  VInst each) instead of `sym_call __lp_lpir_fadd_q32 / fsub_q32`.
- `Q32Options::mul == Wrapping`: emit inline 5-VInst `mul + mulh + srli +
  slli + or` sequence instead of `sym_call __lp_lpir_fmul_q32`.
- `Q32Options::div == Reciprocal`: `sym_call __lp_lpir_fdiv_recip_q32`
  (the helper added in phase 2) instead of `sym_call __lp_lpir_fdiv_q32`.

To do this cleanly, introduce a `LowerOpts<'a>` struct passed by reference
through the lowering call chain.

**Out of scope:**

- Wasm dispatch (phase 4).
- Cranelift dispatch (deprecated, never).
- Filetests (phase 5).
- Helper doc updates ("backends inline this when ...") — phase 6.

## Code Organization Reminders

- One concept per file; keep `LowerOpts` small and well-named (it's a
  carrier for per-call options; just two fields).
- Place new dispatch helpers near related code (probably right above the
  match arm, or in a small private fn next to existing
  `emit_q32_*` helpers in this crate).
- Tests for new arms go in the existing test module of `lower.rs` (or
  whichever file currently tests `lower_lpir_op`). Match the existing
  naming pattern.

## Sub-agent Reminders

- Do **not** commit. The plan commits at the end.
- Do **not** expand scope beyond the four ops listed.
- Do **not** suppress warnings — fix them.
- Do **not** weaken/disable existing tests; they must all keep passing
  with default `Q32Options` (saturating).
- Step 1 is a pure mechanical refactor (LowerOpts threading). Validate
  it (build + all existing tests pass) **before** starting step 2 (new
  dispatch arms). This separation makes debugging trivial if something
  breaks.
- If something blocks completion, stop and report back.
- Report what changed and what was validated.

## Implementation Details

### Step 1: Mechanical refactor — introduce `LowerOpts<'a>`

The current `lower_lpir_op` signature in `lp-shader/lpvm-native/src/lower.rs`
takes `float_mode: FloatMode` directly (read it first). Change to:

```rust
/// Per-call lowering options. Threaded through `lower_lpir_op` and its
/// callees so that fast-math dispatch can read the active `Q32Options`.
#[derive(Clone, Copy)]
pub struct LowerOpts<'a> {
    pub float_mode: lpir::FloatMode,
    pub q32: &'a lps_q32::q32_options::Q32Options,
}
```

Place this near the top of `lower.rs` (or in a new `lower_opts.rs` module
if `lower.rs` already exceeds ~600 lines — check first). If you split it
out, re-export it from `lower.rs` for backwards compat:
`pub use lower_opts::LowerOpts;`.

Update `lower_lpir_op`'s signature to accept `opts: &LowerOpts<'_>` instead
of `float_mode: FloatMode`. Inside, replace direct `float_mode` references
with `opts.float_mode`.

Update **every caller** of `lower_lpir_op` — likely:

- `lp-shader/lpvm-native/src/compile.rs` (the main entry that walks the
  function body)
- Any test setup in `lower.rs#tests`

For test callers, define a small helper:

```rust
fn default_opts() -> (lps_q32::q32_options::Q32Options, ) {
    (lps_q32::q32_options::Q32Options::default(), )
}

// Use:
let q32 = lps_q32::q32_options::Q32Options::default();
let opts = LowerOpts {
    float_mode: lpir::FloatMode::Q32,
    q32: &q32,
};
lower_lpir_op(&mut out, &op, &opts, ...);
```

For `compile.rs`, build `LowerOpts` once per `lower_ops` call (or per
function — whichever scope makes sense given the existing structure):

```rust
let opts = LowerOpts {
    float_mode: session.options.float_mode,
    q32: &session.options.config.q32,
};
// ... pass &opts down ...
```

**Validate before proceeding:**

```bash
cargo build -p lpvm-native
cargo test -p lpvm-native
```

All existing tests must pass. If they don't, the refactor is wrong — fix
before adding new arms.

### Step 2: Add dispatch arms for Fadd/Fsub Wrapping

In the `Op::Fadd` arm of the `lower_lpir_op` match for `FloatMode::Q32`,
the current code looks roughly like:

```rust
(Op::Fadd, FloatMode::Q32) => {
    // sym_call to __lp_lpir_fadd_q32 with operands a, b, dst
}
```

Update to dispatch on `opts.q32.add_sub`:

```rust
(Op::Fadd, FloatMode::Q32) => {
    use lps_q32::q32_options::AddSubMode;
    match opts.q32.add_sub {
        AddSubMode::Saturating => {
            // existing sym_call to __lp_lpir_fadd_q32
        }
        AddSubMode::Wrapping => {
            // 1 VInst: AluRRR { op: AluOp::Add, rd: dst, rs1: a, rs2: b }
            out.push(VInst::AluRRR {
                op: AluOp::Add,
                rd: dst,
                rs1: a,
                rs2: b,
            });
        }
    }
}
```

(Adapt to the actual shape of `VInst::AluRRR` in this crate — read it
from the existing codebase. The crate uses its own `VInst` enum local to
lpvm-native; don't import from `lp-riscv`.)

Same for `Op::Fsub`: `AluOp::Sub`.

### Step 3: Add dispatch arms for Fmul Wrapping (5-VInst sequence)

```rust
(Op::Fmul, FloatMode::Q32) => {
    use lps_q32::q32_options::MulMode;
    match opts.q32.mul {
        MulMode::Saturating => {
            // existing sym_call to __lp_lpir_fmul_q32
        }
        MulMode::Wrapping => {
            // result = ((a as i64 * b as i64) >> 16) as i32
            // RV32: 32x32 -> 64 via mul+mulh, recombine via shifts+or.
            //
            //   mul    lo, a, b      ; lo = bits [31:0]  of a*b
            //   mulh   hi, a, b      ; hi = bits [63:32] of a*b (signed)
            //   srli   lo, lo, 16    ; bits [31:16] -> [15:0]
            //   slli   hi, hi, 16    ; bits [47:32] -> [31:16]
            //   or     dst, lo, hi   ; dst = bits [47:16] of a*b
            let lo = alloc_scratch_reg();  // use the crate's local convention
            let hi = alloc_scratch_reg();
            out.push(VInst::AluRRR { op: AluOp::Mul,  rd: lo, rs1: a, rs2: b });
            out.push(VInst::AluRRR { op: AluOp::MulH, rd: hi, rs1: a, rs2: b });
            out.push(VInst::AluRRI { op: AluOp::Srli, rd: lo, rs1: lo, imm: 16 });
            out.push(VInst::AluRRI { op: AluOp::Slli, rd: hi, rs1: hi, imm: 16 });
            out.push(VInst::AluRRR { op: AluOp::Or,   rd: dst, rs1: lo, rs2: hi });
        }
    }
}
```

**Important — register allocation:** Read how the existing code allocates
scratch registers (search for "scratch" or look at how multi-VInst
expansions like `emit_q32_neg` or similar handle temporaries). Use the
same mechanism. Do NOT clobber `a` or `b` if they may be live afterwards.

If RV32 `mulh` opcode is not present in the crate's `AluOp`, add it
following the existing pattern (add to enum, add encoding match arm). The
RV32IMAC M-extension supports `MUL` (low 32) and `MULH` (high 32 signed).

### Step 4: Add dispatch arm for Fdiv Reciprocal

```rust
(Op::Fdiv, FloatMode::Q32) => {
    use lps_q32::q32_options::DivMode;
    let helper = match opts.q32.div {
        DivMode::Saturating => "__lp_lpir_fdiv_q32",        // existing
        DivMode::Reciprocal => "__lp_lpir_fdiv_recip_q32",  // new (phase 2)
    };
    // sym_call(helper, args=[a, b], ret=dst)
    // ... existing sym_call emit pattern, just with helper name parameterized
}
```

The phase 2 helper must already be registered in `BuiltinTable` for the
sym_call to resolve. Verify by trying a debug build.

### Step 5: Unit tests

Add tests in `lower.rs#tests` (or wherever the existing `lower_lpir_op`
tests live). Pattern: build a small fake op, invoke `lower_lpir_op` with
the relevant `LowerOpts`, assert the produced `Vec<VInst>` matches.

```rust
#[test]
fn fadd_q32_wrapping_emits_inline_add() {
    let q32 = lps_q32::q32_options::Q32Options {
        add_sub: lps_q32::q32_options::AddSubMode::Wrapping,
        ..Default::default()
    };
    let opts = LowerOpts {
        float_mode: lpir::FloatMode::Q32,
        q32: &q32,
    };

    let mut out = Vec::new();
    let op = build_test_fadd(/* a, b, dst as test reg ids */);
    lower_lpir_op(&mut out, &op, &opts, /* ...other args... */);

    assert_eq!(out.len(), 1);
    match &out[0] {
        VInst::AluRRR { op: AluOp::Add, .. } => {}
        other => panic!("expected AluRRR Add, got {other:?}"),
    }
}

#[test]
fn fadd_q32_saturating_emits_sym_call() {
    let q32 = lps_q32::q32_options::Q32Options::default(); // Saturating
    let opts = LowerOpts {
        float_mode: lpir::FloatMode::Q32,
        q32: &q32,
    };

    let mut out = Vec::new();
    let op = build_test_fadd(/*...*/);
    lower_lpir_op(&mut out, &op, &opts, /*...*/);

    assert!(matches_sym_call(&out, "__lp_lpir_fadd_q32"));
}

// Same pattern for:
// - fsub_q32_wrapping_emits_inline_sub
// - fsub_q32_saturating_emits_sym_call
// - fmul_q32_wrapping_emits_5_vinst_sequence (assert the exact
//   mul/mulh/srli/slli/or VInst sequence)
// - fmul_q32_saturating_emits_sym_call
// - fdiv_q32_reciprocal_emits_sym_call_to_recip_helper (assert
//   sym_call name == "__lp_lpir_fdiv_recip_q32")
// - fdiv_q32_saturating_emits_sym_call_to_default (assert
//   sym_call name == "__lp_lpir_fdiv_q32")
```

If a `matches_sym_call` helper doesn't exist, write a trivial one in the
test module.

For the Fmul 5-VInst test, assert the exact opcode sequence and the
correct immediate values (`shamt = 16` for both shifts):

```rust
let kinds: Vec<&str> = out.iter().map(|i| match i {
    VInst::AluRRR { op: AluOp::Mul, .. }   => "mul",
    VInst::AluRRR { op: AluOp::MulH, .. }  => "mulh",
    VInst::AluRRI { op: AluOp::Srli, imm: 16, .. } => "srli16",
    VInst::AluRRI { op: AluOp::Slli, imm: 16, .. } => "slli16",
    VInst::AluRRR { op: AluOp::Or, .. }    => "or",
    other => panic!("unexpected vinst {other:?}"),
}).collect();
assert_eq!(kinds, &["mul", "mulh", "srli16", "slli16", "or"]);
```

### Step 6: Update `compile.rs` to build `LowerOpts` from session options

Already covered in step 1, but double-check the wiring:

```rust
// In CompileSession or its op-lowering loop:
let opts = LowerOpts {
    float_mode: self.options.float_mode,
    q32: &self.options.config.q32,
};
lower_ops(&mut self.code, &self.func.ops, &opts, /* ... */);
```

`NativeCompileOptions::config: CompilerConfig` exists post-phase-1.

## Validate

From workspace root:

```bash
cargo build -p lpvm-native
cargo test -p lpvm-native
cargo build --workspace
```

After step 1: all existing lpvm-native tests pass with no behavior change.
After step 5: all new tests pass; all old tests still pass.

## Definition of done

- `LowerOpts<'a>` struct exists in `lpvm-native` with `float_mode` and
  `q32` fields.
- `lower_lpir_op` signature updated to take `&LowerOpts<'_>`; all callers
  updated.
- `Op::Fadd`/`Op::Fsub` Q32 paths dispatch on `opts.q32.add_sub`:
  Saturating → existing sym_call; Wrapping → `AluRRR { Add | Sub }`.
- `Op::Fmul` Q32 path dispatches on `opts.q32.mul`: Saturating → existing
  sym_call; Wrapping → 5-VInst `mul/mulh/srli/slli/or` sequence.
- `Op::Fdiv` Q32 path dispatches on `opts.q32.div`: Saturating →
  `__lp_lpir_fdiv_q32`; Reciprocal → `__lp_lpir_fdiv_recip_q32`.
- 8 new unit tests (2 per op × 4 ops) all pass.
- All existing `lpvm-native` tests still pass.
- `cargo build --workspace` succeeds; no new warnings.
- `MulH` `AluOp` variant exists (added if missing).
