# Phase 02 — Accumulation: u8 → Q32 LUT

**Sub-agent:** yes (Composer 2)
**Parallel:** —
**Profile after:** yes — `p2-u8lut`

## Scope of phase

Replace `u8_to_q32_normalized`'s `(v * 65536) / 255` divide with a
const-evaluated 256-entry lookup table:

```rust
static U8_TO_Q32: [Q32; 256] = { /* const-eval'd from the same formula */ };
```

The divide currently shows up in profiles as `__divdi3` (~2.0% self) and
`compiler_builtins::int::specialized_div_rem::u64_div_rem` (~2.3% self).
Each non-skip mapping entry calls `u8_to_q32_normalized` 3× (R, G, B), so
this fires very hot.

After this phase, the function becomes a single array index.

**Out of scope:**

- Anything in `runtime.rs`.
- `ChannelLut` (phases 03/04).
- Touching the `u32 * u32 >> 16` multiply (phase 01 already did that).
- Renaming the function or changing its signature beyond what's needed
  to make it inlineable.

## Code organization reminders

- Single file: `lp-core/lp-engine/src/nodes/fixture/mapping/accumulation.rs`.
- Keep `U8_TO_Q32` near the top of the file (after imports) so it's
  visible at a glance — it's a key piece of the file's behaviour, not a
  helper.
- `u8_to_q32_normalized` becomes a thin wrapper (`#[inline]`), retaining
  the same signature so callers don't need to change.
- Tests near the top of `mod tests`; helpers at the bottom.

## Sub-agent reminders

- Do not commit until told to in the **Commit** step.
- Do not expand scope (no `runtime.rs`, no `ChannelLut`, no shader
  changes).
- Do not suppress warnings or `#[allow(...)]`.
- Do not weaken or remove the `u32_mul_matches_i64_reference` test
  added in phase 01.
- The LUT formula must be **bit-exact identical** to the old function.
  If the new test fails, **stop and report** — silently changing the
  formula would break gamma calibration, brightness mapping, and
  downstream perceptual behaviour.
- Report back: files changed, validation output, deviations.

## Implementation details

### The LUT

In `lp-core/lp-engine/src/nodes/fixture/mapping/accumulation.rs`, near
the top of the file (after `use` statements), add:

```rust
/// Lookup table for u8 → Q32 normalization, populated from the exact
/// formula `Q32((v * 65536) / 255)` for v ∈ 0..=255.
///
/// This kills the `__divdi3` / `u64_div_rem` calls that the divide
/// generated on RV32. Bit-exact with the old `u8_to_q32_normalized`
/// formula by construction.
static U8_TO_Q32: [Q32; 256] = {
    let mut table = [Q32::ZERO; 256];
    let mut v = 0usize;
    while v < 256 {
        // Same formula as the old function. Cast chain matches exactly.
        table[v] = Q32(((v as i64) * 65536 / 255) as i32);
        v += 1;
    }
    table
};
```

Note: `const fn` arithmetic on `i64` is stable; `for` loops in const
context are stable on recent toolchains but `while` is the safer
formulation if there's any doubt. The workspace pins nightly so either
works — prefer `while` for portability.

### The function

Replace the existing `u8_to_q32_normalized` body with a single LUT
lookup:

```rust
/// Convert u8 (0–255) from sampler to Q32 (0–1).
///
/// Bit-exact with `Q32(((v as i64) * 65536 / 255) as i32)`; backed by a
/// const-evaluated 256-entry LUT (see `U8_TO_Q32`).
#[inline]
fn u8_to_q32_normalized(v: u8) -> Q32 {
    U8_TO_Q32[v as usize]
}
```

Indexing a `[Q32; 256]` with `v as usize` (where `v: u8`) cannot
out-of-bounds — `v` is `0..=255` and the table has 256 entries — but
LLVM may not always elide the bounds check. If the resulting codegen is
unsatisfying we'll address it in a follow-up; **do not** add `unsafe {
get_unchecked }` in this phase.

### The phase-01 `debug_assert!`

Phase 01 added `debug_assert!(norm_r <= 0x1_0000 && ...)`. The LUT's max
value is `Q32(((255 as i64) * 65536 / 255) as i32) = Q32(65536) =
Q32::ONE`, so `norm.0 ∈ [0, 65536]` still holds. The asserts stay valid
without changes — but do verify by re-running the existing tests, not by
removing them.

### New test (in `mod tests`)

```rust
#[test]
fn u8_to_q32_lut_matches_division_formula() {
    for v in 0u8..=255 {
        let lut = U8_TO_Q32[v as usize].0;
        let formula = ((v as i64) * 65536 / 255) as i32;
        assert_eq!(lut, formula, "LUT mismatch at v={v}");
    }
}

#[test]
fn u8_to_q32_normalized_uses_lut() {
    for v in 0u8..=255 {
        assert_eq!(u8_to_q32_normalized(v), U8_TO_Q32[v as usize]);
    }
}
```

The second test feels redundant but pins the wrapper's contract — if
someone "optimizes" the wrapper later by inlining the formula again,
this test catches it.

## Validate

```bash
cargo test -p lp-engine --lib nodes::fixture::mapping::accumulation
cargo test -p lp-engine --lib nodes::fixture
```

If green:

```bash
cargo test -p lp-engine
cargo clippy -p lp-engine -- -D warnings 2>&1 | rg -A2 accumulation
```

## Commit

After main-agent review:

```bash
git add lp-core/lp-engine/src/nodes/fixture/mapping/accumulation.rs
git commit -m "$(cat <<'EOF'
perf(lp-engine): replace u8→Q32 divide with const-eval LUT

- Replace `(v * 65536) / 255` in u8_to_q32_normalized with a
  const-evaluated U8_TO_Q32: [Q32; 256] table. Bit-exact with the
  divide formula by construction.
- Kills the __divdi3 / u64_div_rem hotspots (~4% combined self-cycles
  in the most recent fastmath profile), called 3× per non-skip
  mapping entry.
- Add tests asserting LUT == formula for all 256 inputs.

Plan: docs/plans/2026-04-19-fixture-render-perf/02-accumulation-u8-q32-lut.md
EOF
)"
```

## Capture profile

```bash
cargo run -p lp-cli --release -- profile examples/perf/fastmath --note p2-u8lut
ls -dt profiles/*--p2-u8lut | head -n 1
```

Report back top 10 from `report.txt`. Specifically check that
`__divdi3` and `u64_div_rem` have shrunk substantially (ideally
disappeared from the top-N).
