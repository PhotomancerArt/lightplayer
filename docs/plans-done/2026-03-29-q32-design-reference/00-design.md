# Design — Q32 design doc + reference implementation

## Scope of work

Write the canonical Q32 design document (`docs/design/q32.md`) and bring the reference
implementation, JIT builtins, and filetests into agreement with it.

## File structure

```
docs/
└── design/
    └── q32.md                              # NEW: single source of truth for Q32 semantics

lp-shader/lps-builtins/src/
├── glsl/q32/types/
│   └── q32.rs                              # UPDATE: saturating operators, fixed div-by-zero,
│                                           #         fixed constant comments
├── builtins/lpir/
│   └── fdiv_q32.rs                         # UPDATE: 0/0 → 0 (currently returns MAX_FIXED)

lp-shader/lps-filetests/filetests/
├── builtins/
│   ├── common-isinf.glsl                   # UPDATE: Q32-specific expectations / @unsupported
│   └── common-isnan.glsl                   # UPDATE: same
└── scalar/float/
    └── op-divide.glsl                      # UPDATE: add div-by-zero edge cases
```

## Conceptual architecture

```
                    docs/design/q32.md
                  (single source of truth)
                           │
          ┌────────────────┼────────────────┐
          ▼                ▼                ▼
   Q32 struct         JIT builtins      Filetests
   (reference)        (extern "C")      (executable proof)

   Operators:         __lp_lpir_f*_q32
   + - * / saturate   saturate + same
   div 0/0 → 0       div-by-zero rules
   div X/0 → ±MAX
                           │
          ┌────────────────┼────────────────┐
          ▼                ▼                ▼
   Cranelift emitter   WASM emitter     LPIR interp
   (must match)        (must match,     (Q32 mode
                        audit deferred)  must match)
```

All consumers trace back to `q32.md`. The design doc describes what Q32 **means**; backends
must conform.

## Key decisions (from question iteration)

1. **`isnan` / `isinf`**: both always `false` on Q32. No NaN/Inf encoding exists.
2. **Div-by-zero**: `0/0 → 0`, `pos/0 → 0x7FFF_FFFF`, `neg/0 → i32::MIN`.
3. **Overflow**: single `Q32` type, all operators saturate to `[i32::MIN, 0x7FFF_FFFF]`.
4. **WASM**: design doc states "must conform"; WASM audit is deferred.

## Design doc outline (`docs/design/q32.md`)

1. **Overview** — What Q32 is, why it exists, relationship to LPIR.
2. **Encoding** — Q16.16 format, range `[-32768.0, 32767.999984741]`, raw `i32` payload.
3. **Conversions** — `from_f32`, `from_i32`, `to_f32`, `to_i32`, `to_u8_clamped`, `to_u16_clamped`,
   `from_fixed`.
4. **Arithmetic** — Saturating add/sub/mul/div/neg/abs. Div-by-zero rules. Rem-by-zero → 0.
5. **Named constants** — `PI`, `TAU`, `E`, `PHI`, `ONE`, `HALF`, `ZERO` with intended float values
   and exact fixed-point representations.
6. **GLSL builtins on Q32** — Table of every GLSL builtin with Q32 behavior (normal case + edge
   case).
7. **Relational builtins** — `isnan` always false, `isinf` always false, comparisons are integer
   comparisons on raw payload.
8. **`@unsupported` policy** — IEEE-only edge tests use `@unsupported(float_mode=q32, …)`.
9. **Backend conformance** — All backends must produce results matching this doc. WASM details
   deferred.
10. **Reference implementation** — Points to `Q32` struct as canonical; JIT builtins as executable
    boundary.
