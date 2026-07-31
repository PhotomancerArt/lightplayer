# ADR: Integer division never traps

- **Status:** Accepted
- **Date:** 2026-07-30
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None

## Context

The contract itself is not new: LPIR integer division and remainder are
specified in `docs/design/lpir/02-core-ops.md` to never trap and to follow
RV32M semantics on the edge cases. What this ADR records is why, because the
reasoning was invisible enough that it failed once already — the Xtensa
backend shipped its bare `QUOS`/`QUOU`/`REMS`/`REMU` instructions with no
guard, and `docs/design/lpir/00-overview.md` said, until today, that a zero
divisor produced `0`, which is not what any implemented backend actually does.
Both mistakes were possible because the "why" lived nowhere durable. It will
be asked again — when F32 mode lands and when the next ISA arrives — so it is
written down once, here.

## Decision

LPIR `idiv_s`, `idiv_u`, `irem_s`, and `irem_u` never trap, and follow RV32M
semantics on the edge cases:

| op | divisor `0` | `i32::MIN` / `-1` |
|---|---|---|
| `idiv_s` | `-1` | `i32::MIN` |
| `idiv_u` | `0xFFFF_FFFF` | n/a |
| `irem_s` | the dividend | `0` |
| `irem_u` | the dividend | n/a |

A backend whose native divide or remainder diverges from this table — by
trapping, faulting, or returning a different value — must emit a guard so the
LPIR-level result matches regardless. This is not a re-decision; it is the
existing contract in `02-core-ops.md`, restated here only so the reasoning
below has something concrete to attach to.

## Alternatives Considered

- **Trap** — the native behavior of Xtensa, WebAssembly, and Cranelift's
  `sdiv`. Rejected: a shader dividing by zero would take down a light
  installation. Where user-authored GLSL runs on device, a bad pixel is an
  acceptable outcome and a reboot is not.
- **Undefined / target-defined.** Rejected: it makes a shader's behavior
  non-portable across the device tier and the browser tier, and it turns a
  correctness question into a per-backend accident — which is exactly how the
  Xtensa gap arose in the first place.
- **Define the result as `0`.** This is what `00-overview.md` wrongly said
  until today. Rejected: it costs a guard on *every* backend, including rv32,
  where the RV32M answer is free in hardware. A contract that taxes the
  reference target to simplify the story is backwards.
- **RV32M semantics (chosen).** Free on the reference target — the device tier
  gets it from hardware at zero cost — and cheap everywhere else. The specific
  values are arbitrary in isolation; what makes the choice right is that it is
  free where the product actually runs.

## Consequences

- A new backend inherits the guard obligation. `IsaTarget::integer_div_traps_on_zero`
  (`lp-shader/lpvm-native/src/isa/mod.rs`) is where it declares its answer, and
  `lp-shader/lps-filetests/filetests/scalar/{int,uint}/op-*-by-zero.glsl` are
  what falsify a wrong one.
- The contract is not annotatable away: a trap on these inputs is a defect,
  not a legitimate target difference. No `@unsupported(xtn.q32)` /
  `@unimplemented(xtn.q32)` may be used to reach green here.
- Guard elision for provably-non-zero divisors is a legitimate optimization
  and does not weaken the contract.
- Known follow-up, real but not done here: in the Xtensa expansion (below),
  the `is_zero` materialization costs 5 of the 8/9 added instructions.
  `Movi(a9,0)` + `BranchRr(Beq,…)` could collapse to a single
  `BranchZ(Beqz)`, and Xtensa's `MOVEQZ`/`MOVNEZ` — already encoded, decoded,
  and emulated in `lp-xt-inst`/`lp-xt-emu` — could shrink the sequence
  further. Worth an emitter-level peephole later.

### The cost, measured

Measured 2026-07-30 by compiling a two-argument `a op b` function for
`IsaTarget::Xtensa` and decoding the emitted machine code with
`lp_xt_inst::decode`. The guard replaces a single 3-byte `QUOS` / `QUOU` /
`REMS` / `REMU` with:

- **divide: 9 Xtensa instructions (27 bytes) — +8 instructions / +24 bytes**
- **remainder: 10 Xtensa instructions (30 bytes) — +9 instructions / +27 bytes**

At the VInst level the expansion is 5 VInsts for divide and 6 for remainder,
replacing 1. The emitted divide guard, verbatim from the decoder (offsets are
within the function):

```
33: Movi(a9, 0)                 ; materialize 0 for the IcmpImm
36: BranchRr(Beq, a13, a9, 5)   ;  ┐
39: Movi(a14, 0)                ;  │ is_zero ∈ {0,1}
42: J(2)                        ;  │
45: Movi(a14, 1)                ;  ┘
48: Rrr(Or,   a13, a13, a14)    ; safe = rhs | is_zero   (1 when rhs == 0)
51: Rrr(Quos, a15, a15, a13)    ; the divide — would have been the only instruction
54: Rt(Neg,   a14, a14)         ; mask = -is_zero  (0 or all-ones)
57: Rrr(Or,   a15, a15, a14)    ; dst = quot | mask
```

Remainder differs only in the tail — `And kept, lhs, mask` then
`Or dst, quot, kept` — because `x % 1 == 0` makes `quot` zero exactly when the
divisor was zero.

Firmware flash cost: +112 bytes on the `fw-esp32c6` image (2 864 528 B →
2 864 640 B; headroom 281 088 B against a 65 536 B margin). That is the
shared-lowering code itself landing in the binary, not a per-division cost —
the C6 is an rv32 target and never executes the guard. Corpus effect:
`xtn.q32` 6303 → 6326 passing cases (+23), `xtlpn.q32` 6356 → 6374 (+18); both
reach 39/39 directives and 3/3 files on `scalar/int/op-divide-by-zero.glsl`,
`scalar/int/op-modulo-by-zero.glsl`, and `scalar/uint/op-divide-by-zero.glsl`.
rv32 and wasm are unchanged at 31587/31587.

**Trapping is not a cheaper point on a spectrum.** The comparison above is
guard-versus-crash, not guard-versus-free — there is no zero-cost option once
a backend's native instruction traps.

The guard set differs per backend, and that is itself part of the decision.
Cranelift guards both the zero divisor and `i32::MIN / -1` because its `sdiv`
traps on both. Xtensa guards only the zero divisor: its divide already yields
`i32::MIN` for `i32::MIN / -1` and `0` for `i32::MIN % -1`, verified in
`lp-xt/lp-xt-emu/src/executor/arith.rs`, which traps only when the divisor is
zero and otherwise falls through to `wrapping_div` / `wrapping_rem`. A uniform
guard on Xtensa would be wasted instructions for a case its hardware already
gets right. Worth saying honestly: the Xtensa oracle here is this repo's own
emulator, whose model claims dual-run parity with silicon, but no hardware
walk gated this particular work.

### Scope

There are **two** div-by-zero contracts in this codebase, and only one of
them is this ADR's:

| | scope | F32 horizon |
|---|---|---|
| **Integer** div/rem by zero (this ADR) | float-mode **independent** — GLSL `int / int` is integer division in Q32 *and* F32 | **survives unchanged** |
| **Float** div by zero (`docs/design/q32.md`, saturate to `0x7FFF_FFFF`) | Q32-only | **disappears** — IEEE gives ±Inf free, no guard, no trap |

The natural instinct — "Q32 is temporary, don't invest in its edge cases" —
is correct about the float saturation table and wrong about the integer
contract. The integer contract outlives Q32 entirely: it is the same code
path, unguarded by float mode, once a board runs real F32.

## Follow-ups

- Emitter-level peephole collapsing the `is_zero` materialization using
  `BranchZ(Beqz)` and/or `MOVEQZ`/`MOVNEZ` (see Consequences above).
