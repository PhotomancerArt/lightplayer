# Phase 1: Rename Builtin Source Functions

## Scope

Rename all `#[unsafe(no_mangle)] pub extern "C" fn` identifiers in
`lps-builtins` to use the new `__lp_<module>_<fn>_<mode>` convention.
This is the source-of-truth rename — everything else derives from these names.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### Q32 math builtins (29 functions, `builtins/q32/`)

Rename each `pub extern "C" fn` identifier. The files themselves don't need
renaming (the generator discovers by walking the directory). Tests within
each file that call the function by name also need updating.

**lpir module** (6 — have matching LPIR opcodes):

| File           | Old name             | New name                 |
|----------------|----------------------|--------------------------|
| `add.rs`       | `__lp_q32_add`       | `__lp_lpir_fadd_q32`     |
| `sub.rs`       | `__lp_q32_sub`       | `__lp_lpir_fsub_q32`     |
| `mul.rs`       | `__lp_q32_mul`       | `__lp_lpir_fmul_q32`     |
| `div.rs`       | `__lp_q32_div`       | `__lp_lpir_fdiv_q32`     |
| `sqrt.rs`      | `__lp_q32_sqrt`      | `__lp_lpir_fsqrt_q32`    |
| `roundeven.rs` | `__lp_q32_roundeven` | `__lp_lpir_fnearest_q32` |

**glsl module** (23 — GLSL std functions, no matching opcode):

| File             | Old name               | New name                |
|------------------|------------------------|-------------------------|
| `sin.rs`         | `__lp_q32_sin`         | `__lps_sin_q32`         |
| `cos.rs`         | `__lp_q32_cos`         | `__lps_cos_q32`         |
| `tan.rs`         | `__lp_q32_tan`         | `__lps_tan_q32`         |
| `asin.rs`        | `__lp_q32_asin`        | `__lps_asin_q32`        |
| `acos.rs`        | `__lp_q32_acos`        | `__lps_acos_q32`        |
| `atan.rs`        | `__lp_q32_atan`        | `__lps_atan_q32`        |
| `atan2.rs`       | `__lp_q32_atan2`       | `__lps_atan2_q32`       |
| `sinh.rs`        | `__lp_q32_sinh`        | `__lps_sinh_q32`        |
| `cosh.rs`        | `__lp_q32_cosh`        | `__lps_cosh_q32`        |
| `tanh.rs`        | `__lp_q32_tanh`        | `__lps_tanh_q32`        |
| `asinh.rs`       | `__lp_q32_asinh`       | `__lps_asinh_q32`       |
| `acosh.rs`       | `__lp_q32_acosh`       | `__lps_acosh_q32`       |
| `atanh.rs`       | `__lp_q32_atanh`       | `__lps_atanh_q32`       |
| `exp.rs`         | `__lp_q32_exp`         | `__lps_exp_q32`         |
| `exp2.rs`        | `__lp_q32_exp2`        | `__lps_exp2_q32`        |
| `log.rs`         | `__lp_q32_log`         | `__lps_log_q32`         |
| `log2.rs`        | `__lp_q32_log2`        | `__lps_log2_q32`        |
| `pow.rs`         | `__lp_q32_pow`         | `__lps_pow_q32`         |
| `inversesqrt.rs` | `__lp_q32_inversesqrt` | `__lps_inversesqrt_q32` |
| `ldexp.rs`       | `__lp_q32_ldexp`       | `__lps_ldexp_q32`       |
| `round.rs`       | `__lp_q32_round`       | `__lps_round_q32`       |
| `fma.rs`         | `__lp_q32_fma`         | `__lps_fma_q32`         |
| `mod_builtin.rs` | `__lp_q32_mod`         | `__lps_mod_q32`         |

For each file: replace all occurrences of the old name with the new name
(function definition, test calls, doc comments referencing it).

### LPFX builtins (67 functions, `builtins/lpfx/`)

Prefix change: `__lpfx_` → `__lp_lpfx_`. The descriptor part stays the same.

For each of the 67 LPFX functions across all subdirectories (color, generative,
math, hash): find-and-replace `__lpfx_` → `__lp_lpfx_` within each file.

Examples:

- `__lpfx_fbm2_q32` → `__lp_lpfx_fbm2_q32`
- `__lpfx_hash_1` → `__lp_lpfx_hash_1`
- `__lpfx_saturate_vec3_q32` → `__lp_lpfx_saturate_vec3_q32`
- `__lpfx_hsv2rgb_vec4_f32` → `__lp_lpfx_hsv2rgb_vec4_f32`

This can be done as a bulk `__lpfx_` → `__lp_lpfx_` replacement across all
files in `builtins/lpfx/`.

### Tests within lps-builtins

Tests in these files call the functions directly. The renames above cover
these since they're in the same files. Verify no test references are missed
by searching for any remaining `__lp_q32_` or `__lpfx_` occurrences in
the `lps-builtins/src/` tree after the rename.

## Validate

After this phase, `lps-builtins` should compile on its own:

```
cargo check -p lps-builtins
cargo test -p lps-builtins
```

Other crates that depend on the old generated names will NOT compile yet
(that's Phase 2). The generator hasn't been updated, so running it would
produce wrong output — don't run it in this phase.
