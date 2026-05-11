# Phase 4: Fixture Produces Control

## Scope Of Phase

Move fixture output from push-buffer side effects to a produced
`ControlProduct` plus control-render capability.

In scope:

- Add fixture runtime state `output: ControlProduct`.
- Make `FixtureNode::tick` publish/update that product.
- Implement `ControlNode` for `FixtureNode`.
- Reuse existing visual materialization and mapping code inside control render.
- Remove `output_sink` from `FixtureNode` construction.

Out of scope:

- Output node consumption.
- Removing `FixtureDef.output_loc` from authored models.
- Full protocol output mapping.

## Code Organization Reminders

- Move public runtime state structs to `lpc-model` when they are part of sync or
  slot exposure.
- Keep fixture-specific mapping helpers under `nodes/fixture`.
- Avoid duplicating the old push path and new render path long-term.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests.
- If blocked, stop and report.

## Implementation Details

Relevant files:

- `lp-core/lpc-model/src/nodes/fixture/`
- `lp-core/lpc-engine/src/nodes/fixture/fixture_node.rs`
- `lp-core/lpc-engine/src/nodes/fixture/mapping/`
- `lp-core/lpc-engine/src/runtime_buffer/runtime_buffer.rs`

Expected changes:

- Fixture state exposes `output` as a slot leaf containing `ControlProduct`.
- `ControlProduct.preferred_extent` should reflect the fixture's natural output
  size from its mapping.
- `FixtureNode::render_control` writes `unorm16` RGB/control samples into the
  target provided by the output.
- Gamma/color order remain fixture responsibilities.
- Interpolation/dithering remain output responsibilities.

## Validate

```bash
cargo test -p lpc-model
cargo test -p lpc-engine nodes::fixture
```
