# P5 — `lpvm-native`: sret driven by `IrFunction.sret_arg`

Plan dir: `docs/plans/2026-04-22-lp-shader-aggregates-m1-pointer-abi-foundation/`
Read first: `00-design.md`, `00-notes.md`.
Depends on: P1 (LPIR sret marker), P3 (frontend now emits aggregate
pointer args + sret).
Parallel with: P4 (`lpvm-cranelift`), P7 (`lpvm-wasm`).

## Scope of phase

Drive the from-scratch RV32 ABI in `lpvm-native` from the explicit
`IrFunction::sret_arg` marker (and `ImportDecl::sret`) instead of the
scalar-count heuristic that exists today. Aggregate pointer args count
as 1 word.

Concretely:

- `lp-shader/lpvm-native/src/abi/classify.rs::scalar_count_of_type`
  treats `IrType::Pointer` as 1 word (it likely already does — confirm)
  and is no longer asked about aggregates (P3 made aggregate args plain
  pointer params).
- `lp-shader/lpvm-native/src/abi/func_abi.rs` (or the rv32-specific
  `isa/rv32/abi.rs`) selects `ReturnMethod::Sret` when
  `func.sret_arg.is_some()`. Drop any "if return count > N" logic.
- The function-prologue VReg → ABI-slot binding accounts for the
  hidden sret param at index 1 (after vmctx).
- The call-emit path passes `[vmctx, sret_addr, ...user_args]` per
  the LPIR arg order; no special case "this call has too many
  returns, allocate caller-side dest slot" — the frontend already
  did that.

**Out of scope:**

- Filetests (P9).
- Host marshalling (P6 covers `lpvm-emu` host side, P8 covers
  `lpvm_abi`).
- Performance work.

## Code organization reminders

- Keep changes scoped to `lp-shader/lpvm-native/src/abi/` and
  `lp-shader/lpvm-native/src/isa/rv32/`.
- Don't add unused helpers.
- Mark transitional code with `// TODO(M1):` comments so P10 can
  audit.

## Sub-agent reminders

- Do **not** commit.
- Stay strictly within `lp-shader/lpvm-native/`.
- Do **not** suppress warnings or add `#[allow(...)]`.
- Do **not** weaken or `#[ignore]` tests.
- If the rv32 ABI needs broader changes than documented (e.g. the
  existing sret machinery needs `a0` to also be returned, or there's
  a calling-convention rule that disagrees), **stop and report**.
- Report back: files changed, validation output, deviations.

## Implementation details

### 1. Inventory the existing rv32 ABI

Read these files first:

- `lp-shader/lpvm-native/src/abi/classify.rs` — has
  `scalar_count_of_type` (or equivalent) used to size return types.
- `lp-shader/lpvm-native/src/abi/func_abi.rs` — defines the ISA-neutral
  `FuncAbi`, `ReturnMethod` (`Direct`, `Sret`, etc.).
- `lp-shader/lpvm-native/src/isa/rv32/abi.rs` — picks the rv32-specific
  `ReturnMethod`. Today it likely uses something like
  `if scalar_count_of_returns > 2 { Sret } else { Direct }`.
- `lp-shader/lpvm-native/src/emit/*` (or the rv32 codegen): the
  function-prologue binding from VRegs to ABI slots, and the call-emit
  that pushes arg registers / spill slots.

Confirm the current heuristic and identify every site it's used.

### 2. Replace the trigger

```rust
// lp-shader/lpvm-native/src/isa/rv32/abi.rs (sketch)

pub fn build_func_abi_rv32(func: &IrFunction) -> FuncAbi {
    let return_method = if func.sret_arg.is_some() {
        ReturnMethod::Sret
    } else {
        // Scalar/vec/mat returns continue to use Direct (a0/a1/...).
        ReturnMethod::Direct
    };
    // ... rest of the abi build (param classification etc.) using
    //     `func.hidden_param_slots()` to skip vmctx + (optional) sret.
    // ...
}
```

For imports, look at the equivalent ABI-build entry (likely
`build_import_abi_rv32` or similar) and read `imp.sret` instead of
the heuristic.

### 3. Function-prologue VReg binding

Wherever the rv32 emitter walks LPIR params and binds them to register
classes / spill slots:

- VReg 0 (vmctx) ↔ `a0`.
- VReg 1 (sret, when set) ↔ `a1`. (Standard RV32 calling convention
  hands sret in `a0`; here we've reserved `a0` for vmctx, so sret rides
  in `a1`. **Confirm against the existing emitter** — if the existing
  sret path put it elsewhere, match that. The point is to keep one
  consistent slot.)
- User params ↔ `a(1 + sret_offset + i)` etc. according to the existing
  arg-passing rules.

Replace any `let user_param_start = 1;` with
`let user_param_start = func.hidden_param_slots() as usize;`.

### 4. Call emission

LPIR `Call.args` already contains `[vmctx, sret?, user_args...]` in the
right order (P3 enforces this). The rv32 call emit walks `Call.args`
and assigns each to its ABI slot. **Do not** insert sret yourself — the
frontend already did. **Do not** allocate a caller-side return buffer
— the frontend already did and stored its address as the sret arg.

If the existing call-emit special-cases `> N` returns by allocating a
caller-side stack slot and inserting a sret pointer arg, **delete that
logic.** The new path is uniform: walk LPIR args, place them, and use
`ReturnMethod::Direct` to read back any direct returns. For sret-using
calls, `ReturnMethod::Sret` reads back nothing (the result lives at
the address the frontend allocated and passed in).

If you're unsure whether some path is dead, add a `debug_assert!` that
the suspect branch is unreachable for sret-marked LPIR functions, run
the tests, and remove it once they're green.

### 5. Tests

Add a small unit test (or extend an existing one) where you build a
trivial LPIR function with `sret_arg = Some(VReg(1))` that stores 4
i32s into `*%1` and rets. Compile through `lpvm-native` and assert:

- `FuncAbi::return_method == ReturnMethod::Sret`.
- The function prologue treats `a1` as the sret pointer (matching the
  existing convention).
- A caller calling that function ends up passing the caller's slot
  address through `a1` per the LPIR `Call.args` layout.

If the existing harness doesn't make hand-built LPIR easy, add a TODO
and defer the unit test to P9; call this out in the report.

## Validate

```
cargo check -p lpvm-native
cargo test  -p lpvm-native
just test-glsl                 # lpvm-native is in this set
```

Filetest-level rv32 behavioural failures (e.g.
`scripts/glsl-filetests.sh --target rv32.q32c`) may exist before P9
re-baselines. Report whether failures are codegen bugs or CHECK-line
mismatches.

## Done when

- Sret trigger reads `sret_arg` / `imp.sret`.
- Heuristic deleted.
- Function-prologue binding uses `func.hidden_param_slots()`.
- Call-emit walks LPIR args directly (no caller-side sret insertion).
- `cargo test -p lpvm-native` is green.
- `just check` is green for this crate.
- No new `#[allow(...)]`; no `#[ignore]`d tests.
