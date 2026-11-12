# Phase 5: Update Common Builtins (Libcall-Based)

## Affected functions

These use `get_math_libcall` or `get_math_libcall_2arg` and need to
dispatch to Q32 builtins when in Q32 mode:

| Function | Current libcall | Q32 builtin |
|----------|----------------|-------------|
| inversesqrt | inversesqrtf | LpQ32Inversesqrt |
| round | roundf | LpQ32Round |
| roundEven | roundevenf | LpQ32Roundeven |
| pow | powf | LpQ32Pow |
| exp | expf | LpQ32Exp |
| log | logf | LpQ32Log |
| exp2 | exp2f | LpQ32Exp2 |
| log2 | log2f | LpQ32Log2 |
| mod | fmodf | LpQ32Mod |

## Change

Same as Phase 4: if Phase 3 branches inside the helpers, these work
automatically. No per-function changes needed.

## Functions that already use NumericMode (no change needed)

These go through `emit_float_*` helpers which dispatch via NumericMode:

- sqrt → `emit_float_sqrt`
- floor → `emit_float_floor`
- ceil → `emit_float_ceil`
- min → `emit_float_min`
- max → `emit_float_max`
- abs → `emit_float_abs`
- clamp → composed from min/max

These already work in Q32 mode once Plan B's Q32Strategy implements
the corresponding methods.

## Functions composed from strategy ops (no change needed)

- fract → `emit_float_floor` + `emit_float_sub` → works via strategy
- mix → `emit_float_mul` + `emit_float_add` → works via strategy
- step → `emit_float_cmp` + select → works via strategy
- smoothstep → composed from arithmetic → works via strategy

## Validate

```bash
cargo check -p lps-compiler --features std
```
