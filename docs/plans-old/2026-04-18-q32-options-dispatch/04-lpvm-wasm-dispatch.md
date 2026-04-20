# Phase 4 — `lpvm-wasm` dispatch

## Scope of phase

Implement `Q32Options`-driven dispatch in `lpvm-wasm` for `Fadd`, `Fsub`,
`Fmul`, `Fdiv` so the browser preview engine produces **bit-identical**
output to `lpvm-native` for the same `(mode, inputs)`. Without this
parity, the preview no longer represents what runs on device.

- Wrapping `Fadd`/`Fsub`: `i32.add` / `i32.sub`.
- Wrapping `Fmul`: `i64.extend_i32_s ×2; i64.mul; i64.const 16; i64.shr_s;
  i32.wrap_i64`.
- Reciprocal `Fdiv`: inline `emit_q32_fdiv_recip` (~15 wasm ops, mirrors
  `__lp_lpir_fdiv_recip_q32` algorithm bit-for-bit).

Wasm does **not** call into `lps-builtins` for any of these — all
expansions are inline.

**Out of scope:**

- lpvm-native dispatch (phase 3).
- Filetests (phase 5).
- Helper doc updates (phase 6).

## Code Organization Reminders

- New emit helpers go in `lp-shader/lpvm-wasm/src/emit/q32.rs` (or
  alongside, in a sibling module if `q32.rs` is getting big — check first
  and only split if it exceeds ~500 lines after additions).
- Dispatch logic stays in `emit/ops.rs` (or wherever the existing
  `Op::Fadd`/etc. match lives).
- Tests in the existing wasm test module(s); follow the existing pattern.

## Sub-agent Reminders

- Do **not** commit. The plan commits at the end.
- Do **not** expand scope.
- Do **not** suppress warnings.
- Do **not** weaken existing tests; defaults must produce identical wasm
  bytes.
- This phase is parallelizable with phase 2 (different crates). It depends
  only on phase 1 (`CompilerConfig::q32`).
- If something blocks completion, stop and report back.

## Implementation Details

### Step 1: Thread `Q32Options` into `EmitCtx`

Read `lp-shader/lpvm-wasm/src/emit/mod.rs` first to find `EmitCtx`. Add a
new field:

```rust
pub struct EmitCtx /* existing fields ... */ {
    // ... existing fields ...
    pub q32: lps_q32::q32_options::Q32Options,
}
```

Find where `EmitCtx` is constructed (likely in a `new` constructor or in
the top-level `emit` entry in `lib.rs`). Set `q32:
options.config.q32.clone()` (or by-value if `Q32Options: Copy` — check;
likely it is since the enums are simple `Copy` types).

`WasmOptions::config: CompilerConfig` exists post-phase-1.

If `EmitCtx` is large or nested (e.g. `FuncEmitCtx { module: &mut EmitCtx,
... }`), make sure `q32` lives at the right level — module-level is fine
since it's read-only and doesn't change between functions in a single
compile. Per-function ctxes can read it via `self.module.q32`.

### Step 2: Add new emit helpers in `emit/q32.rs`

Read `lp-shader/lpvm-wasm/src/emit/q32.rs` first to match conventions
(naming, doc comments, how it pushes wasm ops into the buffer).

#### `emit_q32_fadd_wrap` (1 op)

```rust
/// Inline wrapping Q32 add: `i32.add` (modular arithmetic, no saturation).
/// Matches `lpvm-native` `AluRRR { Add }`. Selected when
/// `Q32Options::add_sub == Wrapping`.
pub fn emit_q32_fadd_wrap(out: &mut /* wasm buf */) {
    out.push_op(WasmOp::I32Add);
}
```

#### `emit_q32_fsub_wrap` (1 op)

```rust
pub fn emit_q32_fsub_wrap(out: &mut /* wasm buf */) {
    out.push_op(WasmOp::I32Sub);
}
```

(Match the actual op naming used in this crate — `WasmOp::I32Add`,
`Instruction::I32Add`, raw byte 0x6A, whatever convention is in use.)

#### `emit_q32_fmul_wrap` (6 ops)

```rust
/// Inline wrapping Q32 multiply: `((a as i64 * b as i64) >> 16) as i32`,
/// modular semantics. Matches `lpvm-native`'s 5-VInst `mul/mulh/srli/slli/or`
/// expansion bit-for-bit.
pub fn emit_q32_fmul_wrap(out: &mut /* wasm buf */) {
    // Stack on entry: ... a, b
    // We need to widen each operand to i64 separately, then multiply.
    // The cleanest sequence assumes `a` and `b` are already on the stack.
    // We need: i64.extend_i32_s applied to each → multiply → shift → wrap.

    // If the typical ABI in this crate emits operands then the op, we can
    // do this with a bit of stack manipulation, OR use temporaries
    // (local.set/local.get) to widen each independently.

    // Simplest, no-temporaries version:
    //   stack: a, b
    //   need:  i64(a), i64(b)
    //   but i64.extend_i32_s only widens the top of the stack, so we
    //   need to swap, widen, swap, widen — wasm has no native swap.
    //
    // Practical: use a scratch i64 local.

    // ... see implementation note below.
}
```

**Implementation note:** Wasm has no swap instruction. There are two
common patterns:

1. **Use a scratch i64 local.** Pop b, widen, store to local; widen a;
   load b; multiply.
2. **Widen at construction time.** If the caller already widened the
   operands, just do `i64.mul; i64.const 16; i64.shr_s; i32.wrap_i64`.

Look at how the existing `emit_q32_fmul` (saturating) handles this — it
needs the same i64 multiplication and likely already has the right
pattern. Mirror that approach, just stop at the `>> 16; wrap` step
without the saturation clamp.

The i64 local approach (showing the conceptual sequence):

```text
;; assume operands a (i32), b (i32) are on the stack
local.set $tmp_b_i32     ;; pop b
local.set $tmp_a_i32     ;; pop a
local.get $tmp_a_i32
i64.extend_i32_s
local.get $tmp_b_i32
i64.extend_i32_s
i64.mul
i64.const 16
i64.shr_s
i32.wrap_i64
```

If an i32 scratch local is needed, follow the existing crate convention
for allocating per-function scratch locals (search for `scratch`,
`alloc_local`, or how `emit_q32_fmul` does its temporaries).

#### `emit_q32_fdiv_recip` (~15 ops)

This must mirror `__lp_lpir_fdiv_recip_q32` (phase 2) bit-for-bit. The
algorithm (signed Q16.16):

```text
if divisor == 0:
    return  0          if dividend == 0
            MAX_FIXED  if dividend > 0
            MIN_FIXED  if dividend < 0
result_sign = -1 if (dividend ^ divisor) < 0 else 1
recip = 0x8000_0000u32 / |divisor|       (i32 udiv)
quot  = (|dividend| as u64 * recip as u64 * 2) >> 16   (truncated to i32)
return quot * result_sign
```

In wasm:

```text
;; entry: stack = dividend(i32), divisor(i32)
local.set $divisor
local.set $dividend

;; ----- divisor == 0 branch -----
local.get $divisor
i32.eqz
if (result i32)
    local.get $dividend
    i32.const 0
    i32.eq
    if (result i32)
        i32.const 0                          ;; 0/0 = 0
    else
        local.get $dividend
        i32.const 0
        i32.gt_s
        if (result i32)
            i32.const 0x7FFFFFFF             ;; MAX_FIXED
        else
            i32.const 0x80000000             ;; MIN_FIXED (i.e., i32::MIN)
        end
    end
else
    ;; ----- non-zero divisor: reciprocal multiply -----
    ;; result_sign = (dividend ^ divisor) < 0 ? -1 : 1
    local.get $dividend
    local.get $divisor
    i32.xor
    i32.const 0
    i32.lt_s
    if (result i32)
        i32.const -1
    else
        i32.const 1
    end
    local.set $sign

    ;; abs values: use (x XOR (x>>31)) - (x>>31) trick or call helper.
    ;; Most readable: branch on sign.
    ;;   abs_x = if x < 0 { 0u32.wrapping_sub(x as u32) } else { x as u32 }
    ;; In wasm: abs(x) = (x ^ (x >> 31)) - (x >> 31)  [signed shift]
    ;;   gives wrapping abs (i32::MIN -> i32::MIN, harmless for our use).

    local.get $dividend
    local.get $dividend
    i32.const 31
    i32.shr_s
    i32.xor
    local.get $dividend
    i32.const 31
    i32.shr_s
    i32.sub
    local.set $abs_dividend

    local.get $divisor
    local.get $divisor
    i32.const 31
    i32.shr_s
    i32.xor
    local.get $divisor
    i32.const 31
    i32.shr_s
    i32.sub
    local.set $abs_divisor

    ;; recip = 0x80000000 / abs_divisor (unsigned div)
    i32.const 0x80000000           ;; (i32 representation of 0x80000000u32)
    local.get $abs_divisor
    i32.div_u
    local.set $recip

    ;; quot = (abs_dividend as u64 * recip as u64 * 2) >> 16
    local.get $abs_dividend
    i64.extend_i32_u
    local.get $recip
    i64.extend_i32_u
    i64.mul
    i64.const 1
    i64.shl                         ;; * 2
    i64.const 16
    i64.shr_u
    i32.wrap_i64
    local.set $quot

    ;; return quot * sign
    local.get $quot
    local.get $sign
    i32.mul
end
```

This is 15-ish ops + locals. Allocate the new wasm locals (`$divisor`,
`$dividend`, `$sign`, `$abs_dividend`, `$abs_divisor`, `$recip`, `$quot`)
using whatever per-function scratch-local mechanism the crate has. If that
mechanism is heavy, an alternative is to compute and inline more
aggressively without intermediate locals — but the readable version above
is fine.

**Verify bit-identity** with the native helper using the unit test
strategy below.

### Step 3: Update `emit/ops.rs` dispatch

Find the `Op::Fadd | Op::Fsub | Op::Fmul | Op::Fdiv` arms (or the single
arm that handles all four — depends on existing structure). Each currently
calls something like `emit_q32_fadd(ctx)`. Update to dispatch on
`ctx.q32.*`:

```rust
Op::Fadd if ctx.float_mode == FloatMode::Q32 => {
    use lps_q32::q32_options::AddSubMode;
    match ctx.q32.add_sub {
        AddSubMode::Saturating => emit_q32_fadd(ctx),
        AddSubMode::Wrapping   => emit_q32_fadd_wrap(ctx),
    }
}
Op::Fsub if ctx.float_mode == FloatMode::Q32 => {
    use lps_q32::q32_options::AddSubMode;
    match ctx.q32.add_sub {
        AddSubMode::Saturating => emit_q32_fsub(ctx),
        AddSubMode::Wrapping   => emit_q32_fsub_wrap(ctx),
    }
}
Op::Fmul if ctx.float_mode == FloatMode::Q32 => {
    use lps_q32::q32_options::MulMode;
    match ctx.q32.mul {
        MulMode::Saturating => emit_q32_fmul(ctx),
        MulMode::Wrapping   => emit_q32_fmul_wrap(ctx),
    }
}
Op::Fdiv if ctx.float_mode == FloatMode::Q32 => {
    use lps_q32::q32_options::DivMode;
    match ctx.q32.div {
        DivMode::Saturating => emit_q32_fdiv(ctx),
        DivMode::Reciprocal => emit_q32_fdiv_recip(ctx),
    }
}
```

(Adapt match-guard syntax to whatever the existing arm looks like — this
is purely illustrative.)

### Step 4: Unit tests

Wasm tests in this crate likely either (a) execute the wasm via wasmtime
and assert the runtime result, or (b) assert bytes/instruction-sequence.
Match whichever pattern existing tests use.

For each new emit fn, add tests:

- `fadd_q32_wrap_emits_i32_add` — check wasm output contains `I32Add`
  (or runs and produces expected wrapping result for `(MAX_FIXED, 1)`).
- `fadd_q32_saturating_unchanged` — check default still emits the
  saturating path.
- Same shape for `fsub`, `fmul`, `fdiv`.

For `fdiv_recip` specifically, add a runtime-correctness test (using
wasmtime if the crate already uses it for tests):

```rust
#[test]
fn fdiv_recip_matches_native_helper() {
    let cases: &[(i32, i32)] = &[
        (float_to_fixed(10.0), float_to_fixed(2.0)),
        (float_to_fixed(-10.0), float_to_fixed(2.0)),
        (float_to_fixed(10.0), float_to_fixed(-2.0)),
        (float_to_fixed(-10.0), float_to_fixed(-2.0)),
        (float_to_fixed(1.5), float_to_fixed(0.25)),
        (float_to_fixed(0.0), float_to_fixed(1.0)),
        (float_to_fixed(1.0), 0),                       // div by zero
        (float_to_fixed(-1.0), 0),                      // div by zero
        (0, 0),                                          // 0/0
    ];
    for &(a, b) in cases {
        let expected = lps_builtins::builtins::lpir::fdiv_recip_q32::__lp_lpir_fdiv_recip_q32(a, b);
        let got = run_wasm_fdiv_recip(a, b);
        assert_eq!(got, expected, "fdiv_recip({a}, {b}): wasm={got} native={expected}");
    }
}
```

If `lpvm-wasm` doesn't currently link against `lps-builtins` for tests,
either add `lps-builtins = { path = "..." }` as a `[dev-dependencies]`
entry, or copy the algorithm in pure Rust into the test for cross-check.
The first option is simpler and ensures genuine bit-identity.

Add similar `fmul_q32_wrap_matches_native_arith` test that, given a few
representative `(a, b)` inputs, computes
`((a as i64 * b as i64) >> 16) as i32` in pure Rust and asserts the wasm
runtime produces the same value.

### Step 5: Update `WasmOptions` if needed

`WasmOptions::config: CompilerConfig` should already exist post-phase-1.
Just confirm it does and that `EmitCtx::new` (or equivalent) reads
`options.config.q32`.

## Validate

```bash
cargo build -p lpvm-wasm
cargo test -p lpvm-wasm
cargo build --workspace
```

All new tests pass; all existing wasm tests pass; defaults produce
identical wasm output (regression-test the default path explicitly).

## Definition of done

- `EmitCtx` has a `q32: Q32Options` field, populated from
  `WasmOptions::config.q32`.
- New helpers in `emit/q32.rs`: `emit_q32_fadd_wrap`, `emit_q32_fsub_wrap`,
  `emit_q32_fmul_wrap`, `emit_q32_fdiv_recip`.
- `emit/ops.rs` dispatches `Fadd`/`Fsub`/`Fmul`/`Fdiv` on the appropriate
  `ctx.q32.*` field.
- Unit tests for both modes of all four ops; `fdiv_recip` and
  `fmul_wrap` have runtime-correctness tests asserting bit-identity with
  the native algorithm.
- All existing `lpvm-wasm` tests still pass.
- `cargo build --workspace` succeeds; no new warnings.
