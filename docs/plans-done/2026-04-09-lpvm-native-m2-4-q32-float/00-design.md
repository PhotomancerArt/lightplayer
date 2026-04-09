# M2.4 Q32 Float Operations - Design

## Scope of Work

Implement Q32 float support in lpvm-native via soft-float builtins and integer comparisons:

- **Q32 arithmetic**: fadd, fsub, fmul, fdiv (fadd/fsub/fmul already done)
- **Q32 comparisons**: feq, fne, flt, fle, fgt, fge via integer compares
- **Q32 division**: fdiv via `__lp_lpir_fdiv_q32` builtin call
- **Tests**: Unit tests in lower.rs

## File Structure

```
lp-shader/lpvm-native/src/
└── lower.rs              # UPDATE: Add Fdiv + 6 comparison lowerings
                            # Lines ~300: Add new Op match arms
                            # Tests: Add unit tests

Existing filetests (already cover Q32):
lp-shader/lps-filetests/filetests/
├── scalar/float/         # float ops via Q32
├── vec2/, vec3/, vec4/  # vector ops via Q32
└── mat2/, mat3/, mat4/  # matrix ops via Q32
```

## Conceptual Architecture

Q32 lowering happens entirely in the LPIR → VInst layer. VInst remains unaware of Q32 semantics.

```
┌─────────────────────────────────────────────────────────┐
│  LPIR Float Op (FloatMode::Q32)                          │
│                                                          │
│  ├─► Fadd ──► VInst::Call("__lp_lpir_fadd_q32")          │
│  ├─► Fsub ──► VInst::Call("__lp_lpir_fsub_q32")          │
│  ├─► Fmul ──► VInst::Call("__lp_lpir_fmul_q32")          │
│  ├─► Fdiv ──► VInst::Call("__lp_lpir_fdiv_q32")  [NEW]  │
│                                                          │
│  ├─► Feq ───► VInst::Icmp32(cond: Eq)           [NEW]   │
│  ├─► Fne ───► VInst::Icmp32(cond: Ne)           [NEW]   │
│  ├─► Flt ───► VInst::Icmp32(cond: LtS)          [NEW]   │
│  ├─► Fle ───► VInst::Icmp32(cond: LeS)          [NEW]   │
│  ├─► Fgt ───► VInst::Icmp32(cond: GtS)          [NEW]   │
│  └─► Fge ───► VInst::Icmp32(cond: GeS)          [NEW]   │
│                                                          │
│  ├─► FconstF32 ──► VInst::IConst32(val * 65536.0)        │
│                                                          │
│  F32 mode (not supported):                               │
│  └─► Err("float op requires Q32 mode...")                │
└─────────────────────────────────────────────────────────┘
```

### Q32 Comparison Mapping (matches cranelift)

| LPIR Op | VInst | Notes |
|---------|-------|-------|
| Feq | Icmp32(Eq) | Integer equality works for Q32 |
| Fne | Icmp32(Ne) | Integer inequality |
| Flt | Icmp32(LtS) | Signed less than (Q32 is signed fixed-point) |
| Fle | Icmp32(LeS) | Signed less or equal |
| Fgt | Icmp32(GtS) | Signed greater than |
| Fge | Icmp32(GeS) | Signed greater or equal |

## Main Components

### lower.rs

The `lower_op` function matches on `Op` and produces `VInst`. For Q32 mode:

- **Arithmetic ops** (Fadd, Fsub, Fmul, Fdiv): Lower to `VInst::Call` with builtin symbol
- **Comparison ops** (Feq, Fne, Flt, Fle, Fgt, Fge): Lower to `VInst::Icmp32` with appropriate condition
- **Constants** (FconstF32): Lower to `VInst::IConst32` with scaled value

### lps-builtins

The runtime provides:
- `__lp_lpir_fadd_q32(a: i32, b: i32) -> i32`
- `__lp_lpir_fsub_q32(a: i32, b: i32) -> i32`
- `__lp_lpir_fmul_q32(a: i32, b: i32) -> i32`
- `__lp_lpir_fdiv_q32(a: i32, b: i32) -> i32` (handles 0/0 → 0)

These are already implemented and tested.

## Acceptance Criteria

1. Fdiv lowers to `__lp_lpir_fdiv_q32` call in Q32 mode
2. All 6 float comparisons lower to integer compares in Q32 mode
3. F32 mode returns clear error for all float ops
4. Unit tests verify each lowering
5. Existing float filetests pass (scalar/float, vecX, matX)
