# Phase 2: Float comparison lowerings (Q32)

## Scope of phase

Lower `Op::Feq`, `Fne`, `Flt`, `Fle`, `Fgt`, `Fge` when `float_mode == FloatMode::Q32` to the same `VInst::Icmp32` conditions as the Cranelift backend uses for Q32 (signed integer compare on fixed-point bits).

Reference: `lp-shader/lpvm-cranelift/src/emit/scalar.rs` (Feq → Equal, Fne → NotEqual, Flt → SignedLessThan, etc.).

## Match semantics (F32 mode)

Same as Phase 1: each `Op::Feq` / … arm with `if float_mode == FloatMode::Q32` does **not** match in F32 mode. **Phase 3** must list all six ops on the F32 catch-all so they never hit `other =>`.

## Code Organization Reminders

- Group all six Q32 float comparison arms together (after Fdiv Q32, before FconstF32).
- Reuse existing `IcmpCond` variants; do not add Q32-specific VInst variants.

## Implementation Details

In `lower_op`, for each op when `float_mode == FloatMode::Q32`, return `VInst::Icmp32` with:

| Op   | `IcmpCond` |
|------|------------|
| Feq  | `Eq`       |
| Fne  | `Ne`       |
| Flt  | `LtS`      |
| Fle  | `LeS`      |
| Fgt  | `GtS`      |
| Fge  | `GeS`      |

Each arm: `dst`, `lhs`, `rhs` from the LPIR op; `src_op` passed through.

### Tests

Add one test per op (or one parameterized-style test with six cases) asserting the correct `IcmpCond` for Q32 mode, e.g. `lower_q32_feq_to_icmp_eq`, `lower_q32_flt_to_icmp_lts`, etc.

## Validate

```bash
cargo test -p lpvm-native --lib lower_q32_f
cargo +nightly fmt -p lpvm-native
```
