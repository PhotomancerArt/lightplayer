# Phase 5: Direct Sampling Strategy

## Scope Of Phase

Implement direct fixture sampling.

In scope:
- Add `nodes/fixture/sampling/direct.rs`.
- Precompute Q16.16 lamp sample points from fixture mapping.
- Reuse RGBA16 sample scratch buffer.
- Ask visual product owner to sample points.
- Convert RGBA16 samples to output-owned unorm16 control samples with fixture brightness, gamma, and color order.

Out of scope:
- Supersampling/anti-aliasing.
- Direct-to-control shader synthetic function.
- Removing texture-area strategy.

## Code Organization Reminders

- Keep direct strategy code separate from texture-area strategy.
- Keep hot-path buffers fixed-point/unorm16.
- Avoid per-frame allocations once cache sizes are stable.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and deviations.

## Implementation Details

Relevant files:
- `lp-core/lpc-engine/src/nodes/fixture/sampling/direct.rs`
- `lp-core/lpc-engine/src/nodes/fixture/sampling/points.rs`
- `lp-core/lpc-engine/src/nodes/fixture/fixture_node.rs`
- engine visual sampling APIs from Phase 2

Expected tests:
- Direct fixture samples deterministic shader and writes expected control data.
- Direct strategy does not allocate a fixture render texture.
- Texture-area strategy still works.

## Validate

```bash
cargo fmt --check
cargo test -p lpc-engine fixture -- --nocapture
cargo test -p lpc-engine output_ -- --nocapture
```

