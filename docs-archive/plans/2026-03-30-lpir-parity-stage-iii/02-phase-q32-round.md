# Phase 2: Q32 Round Builtin

## Scope of phase

Promote `round` from "not yet implemented" to implemented in Q32 spec, verify lowering path exists,
and remove test annotation.

## Code organization reminders

- Update `docs/design/q32.md` builtins table
- Verify builtin is already registered in lowering
- Remove `@unimplemented` annotation from test

## Implementation details

### Step 1: Update Q32 spec

Edit `docs/design/q32.md` §5 "Named Constants":

1. Move `round` from "Builtins not yet implemented" to the main table
2. Add row:
   | Builtin | Q32 behavior |
   |---------|--------------|
   | `round(x)` | Round to nearest, halfway cases away from zero |

3. Reference implementation: `__lps_round_q32` in
   `lps-builtins/src/builtins/glsl/round_q32.rs`

### Step 2: Verify lowering path

Check if `round` is already wired in lowering:

```bash
grep -n "round" lp-shader/lps-frontend/src/lower*.rs
```

If missing, add to `lower_math.rs` or appropriate location to map `MathFunction::Round` to
`__lps_round_q32`.

### Step 3: Update test annotation

Edit `lp-shader/lps-filetests/filetests/const/builtin/extended.glsl`:

Remove line 15-16:

```glsl
// @unimplemented(backend=jit)
// @unimplemented(backend=wasm)
```

## Validate

```bash
# Run the specific test
cd /Users/yona/dev/photomancer/lp2025
./scripts/filetests.sh --target jit.q32 "const/builtin/extended.glsl:17"

# Expected: passes with round(2.5) = 3.0 (not 2.0)

# Run full extended test file
./scripts/filetests.sh --target jit.q32 "const/builtin/extended.glsl"

# Verify roundEven still works (different builtin)
./scripts/filetests.sh --target jit.q32 "builtins/common-roundeven.glsl"

# Check no regressions
./scripts/filetests.sh --target jit.q32 "builtins/common-round.glsl"
```
