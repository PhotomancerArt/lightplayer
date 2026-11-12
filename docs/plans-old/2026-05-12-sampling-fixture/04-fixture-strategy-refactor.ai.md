# Phase 4: Fixture Strategy Refactor

## Scope Of Phase

Move existing texture/polygon fixture behavior into a strategy module without changing behavior.

In scope:
- Add `nodes/fixture/sampling/texture_area.rs`.
- Add shared point-generation module for both strategies.
- Keep current fixture output behavior and tests green.

Out of scope:
- Direct shader sampling runtime behavior.
- Example conversion.

## Code Organization Reminders

- Keep `fixture_node.rs` as orchestration, not a dumping ground.
- Use `sampling/points.rs` for logical lamp point generation.
- Keep old polygon mapping code under `mapping/`.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and deviations.

## Implementation Details

Relevant files:
- `lp-core/lpc-engine/src/nodes/fixture/fixture_node.rs`
- `lp-core/lpc-engine/src/nodes/fixture/mapping/*`
- `lp-core/lpc-engine/src/nodes/fixture/sampling/*`

Expected changes:
- Preserve current texture-area behavior with `FixtureSamplingConfig::TextureArea`.
- Move texture render target and `PixelMappingEntry` cache into texture-area state where practical.
- Reuse existing `render_fixture_control_target` or equivalent shared output-writing helper.

## Validate

```bash
cargo fmt --check
cargo test -p lpc-engine fixture -- --nocapture
cargo test -p lpc-engine output_ -- --nocapture
```

