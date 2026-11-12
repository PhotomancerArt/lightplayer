# Phase 01 — Accumulation: u32 multiply

**Sub-agent:** yes (Composer 2)
**Parallel:** —
**Profile after:** yes — `p1-u32mul`

## Scope of phase

Replace the `i64 * i64 >> 16` per-RGB multiply in
`accumulate_from_mapping` (file:
`lp-core/lp-engine/src/nodes/fixture/mapping/accumulation.rs`,
lines ~117–119) with a `u32 * u32 >> 16` multiply. On RV32 this kills
the codegen for an `i64` multiply (currently `mul`+`mulhu`+compose) and
reduces it to a single `mul`.

The math is bit-exact — both operands fit in `u32`, the product fits in
`u32`, and the right shift by 16 produces the same `i32` low bits. A new
test pins this behaviour to defend against future drift.

**Out of scope:**

- Phase 02's `u8 → Q32` LUT (the `/255` divide in `u8_to_q32_normalized`
  stays as-is for this phase).
- Phase 03/04's `ChannelLut` integration.
- Any change to `runtime.rs`.
- Any change to `entry.rs`, `points.rs`, `sampling/`, or `overlap/`.
- "While I'm here" cleanups inside `accumulation.rs` — leave the rest of
  the file alone.

## Code organization reminders

- Granular files (one concept per file): we already have one file for
  accumulation; just edit it in place.
- Tests near the top of the file via `#[cfg(test)] mod tests` (sibling
  files in the directory follow this pattern).
- Helpers at the bottom.
- Mark anything temporary with `TODO`. (Nothing temporary expected
  here — this is a final change.)

## Sub-agent reminders

- Do not commit until told to in the **Commit** step below — wait for
  the main-agent review of the diff first.
- Do not expand scope. Phase 02 will replace the `/255` divide; do not
  preempt it.
- Do not suppress warnings, do not add `#[allow(...)]`, do not weaken
  or skip existing tests.
- If you cannot make the new test pass, **stop and report** — that
  almost certainly means the range analysis is wrong and we need to
  reassess. Do not silently fall back to the old i64 path.
- Report back: files changed, validation output, and any deviations.

## Implementation details

### Range analysis (used to justify the u32 multiply)

- `entry.contribution_raw()` returns a `u32` masked with `0xFFFF`
  (`mapping/entry.rs:61`), so it is in `[0, 65535]`. The multiply branch
  is gated on `contribution_raw != 0`, so `frac.0 ∈ [1, 65535]`.
- Until phase 02 lands, `u8_to_q32_normalized(v)` returns
  `Q32(((v as i64) * 65536 / 255) as i32)`, max value at `v=255` is
  `255 * 65536 / 255 = 65536`. So `norm.0 ∈ [0, 65536]` (note: 65536,
  not 65535 — this matters for the `debug_assert!`).
- Product: `65536 * 65535 = 4_294_901_760`, which fits in `u32::MAX =
  4_294_967_295`. Just under the limit. Worth asserting.

After phase 02 lands, `norm.0` will be in `[0, 65536]` still (LUT
preserves the same mapping). Either way, `u32` suffices.

### The change

In `lp-core/lp-engine/src/nodes/fixture/mapping/accumulation.rs`,
inside the `else` branch starting at line ~110, replace:

```rust
let frac = Q32(contribution_raw);
let norm_r = u8_to_q32_normalized(pixel_r);
let norm_g = u8_to_q32_normalized(pixel_g);
let norm_b = u8_to_q32_normalized(pixel_b);

// Q32 multiplication: (a.0 * b.0) >> 16
let accumulated_r = Q32(((norm_r.0 as i64 * frac.0 as i64) >> 16) as i32);
let accumulated_g = Q32(((norm_g.0 as i64 * frac.0 as i64) >> 16) as i32);
let accumulated_b = Q32(((norm_b.0 as i64 * frac.0 as i64) >> 16) as i32);
```

with:

```rust
let frac = contribution_raw as u32;          // [1, 65535]
let norm_r = u8_to_q32_normalized(pixel_r).0 as u32; // [0, 65536]
let norm_g = u8_to_q32_normalized(pixel_g).0 as u32;
let norm_b = u8_to_q32_normalized(pixel_b).0 as u32;

debug_assert!(frac <= 0xFFFF);
debug_assert!(
    norm_r <= 0x1_0000 && norm_g <= 0x1_0000 && norm_b <= 0x1_0000,
    "norm exceeds Q32 ONE; was the u8 LUT changed?",
);

// Q32 multiplication: (a.0 * b.0) >> 16. Both operands fit in u32 and
// the product (~4.295e9 max) fits in u32::MAX, so no i64 needed.
let accumulated_r = Q32(((norm_r * frac) >> 16) as i32);
let accumulated_g = Q32(((norm_g * frac) >> 16) as i32);
let accumulated_b = Q32(((norm_b * frac) >> 16) as i32);
```

Note: `contribution_raw` is currently bound as `i32`
(line `let contribution_raw = entry.contribution_raw() as i32;`). Bind
it as `u32` instead, or cast at use site — pick whichever is cleaner.
The original cast to `i32` was only needed to feed `Q32(...)`; now that
we use the value directly as a `u32` operand, the `i32` cast is
unnecessary.

Recommended: change the binding so it's clearly a `u32`:

```rust
let contribution_raw = entry.contribution_raw(); // already u32
```

…and remove the now-unused `i32` cast. This avoids a `u32 → i32 → u32`
roundtrip.

### New test (in same file's `mod tests`)

Add a test that pins bit-exact equality with the old `i64` formula for
all interesting `(norm, frac)` pairs:

```rust
#[test]
fn u32_mul_matches_i64_reference() {
    // Reference: the old i64 path.
    fn i64_ref(norm: i32, frac: i32) -> i32 {
        ((norm as i64 * frac as i64) >> 16) as i32
    }

    // Walk every u8 input through u8_to_q32_normalized so we cover the
    // exact `norm` values the production path produces.
    for v in 0u8..=255 {
        let norm = u8_to_q32_normalized(v).0;
        for &frac in &[1i32, 2, 100, 1000, 0x4000, 0x8000, 0xC000, 0xFFFF] {
            let new = ((norm as u32 * frac as u32) >> 16) as i32;
            let old = i64_ref(norm, frac);
            assert_eq!(
                new, old,
                "mismatch at norm={norm} (v={v}), frac={frac}"
            );
        }
    }
}
```

(Tests live in `mod tests { use super::*; ... }`; if the file does not
yet have a tests module, add one — see sibling files like `entry.rs`
for the pattern.)

## Validate

```bash
cargo test -p lp-engine --lib nodes::fixture::mapping::accumulation
cargo test -p lp-engine --lib nodes::fixture
```

The first targets the new test directly; the second catches any
regression in adjacent fixture tests. If both are green, run the full
crate suite once for safety:

```bash
cargo test -p lp-engine
```

No need to run the full `just check` here — phase 05 does the
project-wide validation. But do confirm clippy is clean for the file
you touched:

```bash
cargo clippy -p lp-engine -- -D warnings 2>&1 | rg -A2 accumulation
```

## Commit

After main-agent review approves the diff:

```bash
git add lp-core/lp-engine/src/nodes/fixture/mapping/accumulation.rs
git commit -m "$(cat <<'EOF'
perf(lp-engine): use u32 multiply in fixture accumulation hot path

- Replace `i64 * i64 >> 16` with `u32 * u32 >> 16` in
  accumulate_from_mapping. Both operands and product fit in u32; this
  drops to a single `mul` on RV32 instead of `mul`+`mulhu`+compose.
- Add debug_asserts pinning the range invariants.
- Add u32_mul_matches_i64_reference test asserting bit-exact equality
  with the old i64 formula across all 256 u8 inputs and 8 frac values.

Plan: docs/plans/2026-04-19-fixture-render-perf/01-accumulation-u32-mul.md
EOF
)"
```

## Capture profile

```bash
cargo run -p lp-cli --release -- profile examples/perf/fastmath --note p1-u32mul
ls -dt profiles/*--p1-u32mul | head -n 1
```

Read `report.txt` in that dir; report back the top 10 entries plus
specifically the lines for `accumulate_from_mapping` and any
`compiler_builtins` symbols (so we can confirm the i64 helpers shrink).
