# Phase 1 — Lower API refactor

## Scope of phase

Lift the 1-LPIR-op → 1-VInst constraint in
`lpvm-native::lower::lower_lpir_op` by switching it to a sink-parameter
API and introducing a `TempVRegs` watermark for fresh intermediate vregs.

**No behavior change.** Every match arm is rewritten mechanically:
`Ok(VInst::Foo {…})` → `out.push(VInst::Foo {…}); Ok(())`. After this
phase, every LPIR op still produces exactly one `VInst`. All tests pass
identically.

## Code organization reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a `TODO` comment so we can find it later.

## Implementation details

### 1.1 Add `TempVRegs` to `vinst.rs`

Place near the top of `lp-shader/lpvm-native/src/vinst.rs`, before `enum
VInst`. Tiny newtype, kept here because it conceptually belongs with
`VReg`.

```rust
/// Watermark allocator for fresh temporary [`VReg`]s during lowering.
///
/// Initialized to `func.vreg_types.len() as u16` (i.e. one past the
/// highest IR-declared vreg). Each [`mint`] call returns a fresh
/// [`VReg`] above the IR vreg space; ids never collide with IR vregs and
/// never reset across LPIR ops within a function.
///
/// Used by [`crate::lower::lower_lpir_op`] when an op expands to
/// multiple [`VInst`]s and needs intermediate registers.
#[derive(Clone, Copy, Debug)]
pub struct TempVRegs(u16);

impl TempVRegs {
    pub fn new(after_ir: u16) -> Self {
        Self(after_ir)
    }

    pub fn mint(&mut self) -> VReg {
        let v = VReg(self.0);
        self.0 = self
            .0
            .checked_add(1)
            .expect("lpvm-native: temp vreg space exhausted (u16)");
        v
    }
}
```

Re-export from `lib.rs` (already re-exports `VReg`, etc).

### 1.2 Rewrite `lower_lpir_op` signature

Change the signature in `lp-shader/lpvm-native/src/lower.rs:62`:

```rust
pub fn lower_lpir_op(
    out: &mut Vec<VInst>,             // NEW: sink param, first for visibility
    op: &LpirOp,
    float_mode: FloatMode,
    src_op: Option<u32>,
    func: &IrFunction,
    ir: &LpirModule,
    abi: &ModuleAbi,
    symbols: &mut ModuleSymbols,
    vreg_pool: &mut Vec<VReg>,
    temps: &mut TempVRegs,            // NEW: temp allocator
) -> Result<(), LowerError> {         // NEW: returns ()
    let po = pack_src_op(src_op);
    match op {
        LpirOp::Iadd { dst, lhs, rhs } => {
            out.push(VInst::AluRRR { op: AluOp::Add, dst: fa_vreg(*dst),
                src1: fa_vreg(*lhs), src2: fa_vreg(*rhs), src_op: po });
            Ok(())
        }
        // ... every other arm rewritten the same way ...
    }
}
```

Mechanical transformation for **every** match arm — `Ok(VInst::X { … })`
becomes `out.push(VInst::X { … }); Ok(())`. The existing `sym_call`
helper also gets the sink-param treatment:

```rust
fn sym_call(
    out: &mut Vec<VInst>,
    symbols: &mut ModuleSymbols,
    pool: &mut Vec<VReg>,
    name: &'static str,
    args: &[lpir::VReg],
    rets: &[lpir::VReg],
    src_op: Option<u32>,
) -> Result<(), LowerError> {
    out.push(VInst::Call {
        target: symbols.intern(name),
        args: push_vregs_slice(pool, args)?,
        rets: push_vregs_slice(pool, rets)?,
        callee_uses_sret: false,
        src_op: pack_src_op(src_op),
    });
    Ok(())
}
```

Don't add `temps` to `sym_call` — it doesn't need temps. Just to
`lower_lpir_op`'s signature.

### 1.3 Update the sole production caller

`lp-shader/lpvm-native/src/lower.rs:1190`:

```rust
let is_return = matches!(other, LpirOp::Return { .. });
lower_lpir_op(
    &mut self.out,             // sink
    other,
    self.float_mode,
    Some(i as u32),
    self.func,
    self.ir,
    self.abi,
    &mut self.symbols,
    &mut self.vreg_pool,
    &mut self.temps,           // NEW field on LoweredFunction
)?;
```

Add `temps: TempVRegs` to the per-function lowering state struct (find
where `self.out`, `self.symbols`, `self.vreg_pool` live; same struct).
Initialize at function entry:

```rust
let temps = TempVRegs::new(func.vreg_types.len() as u16);
```

### 1.4 Update test helpers

`lp-shader/lpvm-native/src/lower.rs:1401-1426` defines `call_lower_op`
and `call_lower_op_full`. Rewrite to return `Vec<VInst>`:

```rust
fn call_lower_op(
    op: &LpirOp,
    float_mode: FloatMode,
    src_op: Option<u32>,
    f: &IrFunction,
    ir: &LpirModule,
    abi: &ModuleAbi,
) -> Result<Vec<VInst>, LowerError> {
    let mut out = Vec::new();
    let mut symbols = ModuleSymbols::default();
    let mut pool = Vec::new();
    let mut temps = TempVRegs::new(f.vreg_types.len() as u16);
    super::lower_lpir_op(
        &mut out, op, float_mode, src_op, f, ir, abi,
        &mut symbols, &mut pool, &mut temps,
    )?;
    Ok(out)
}

fn call_lower_op_full(
    op: &LpirOp,
    float_mode: FloatMode,
    src_op: Option<u32>,
    f: &IrFunction,
    ir: &LpirModule,
    abi: &ModuleAbi,
) -> Result<(Vec<VInst>, ModuleSymbols, Vec<FaVReg>), LowerError> {
    let mut out = Vec::new();
    let mut symbols = ModuleSymbols::default();
    let mut pool = Vec::new();
    let mut temps = TempVRegs::new(f.vreg_types.len() as u16);
    super::lower_lpir_op(
        &mut out, op, float_mode, src_op, f, ir, abi,
        &mut symbols, &mut pool, &mut temps,
    )?;
    Ok((out, symbols, pool))
}
```

### 1.5 Update existing unit tests

The existing tests in `lp-shader/lpvm-native/src/lower.rs:1392-2131`
match on the *single* returned `VInst`. Mechanical update:

```rust
// before:
let v = call_lower_op(&op, FloatMode::Q32, None, &f, &ir, &abi).unwrap();
match v {
    VInst::Call { target, args, .. } => { … }
    other => panic!("unexpected: {other:?}"),
}

// after:
let v = call_lower_op(&op, FloatMode::Q32, None, &f, &ir, &abi).unwrap();
assert_eq!(v.len(), 1, "phase 1: every op still emits exactly 1 VInst");
match &v[0] {
    VInst::Call { target, args, .. } => { … }
    other => panic!("unexpected: {other:?}"),
}
```

The `assert_eq!(v.len(), 1, ...)` is **temporary** — it'll be wrong for
inlined ops in Phase 2/3. Mark with `// TODO(phase-2): drop len assertion
for inlined ops`. Phase 2/3 will rewrite the per-op assertions one at a
time, dropping the length assertion as each becomes multi-VInst.

## Validate

```
turbo check test
```

Must pass cleanly. This phase is a no-op semantically — every test
should produce identical results.

If any non-test caller of `lower_lpir_op` exists outside the file,
update it identically (use Grep for `lower_lpir_op` in the workspace to
confirm — at the time of writing, only `src/lib.rs` re-exports it and
`lower_ops` calls it).
