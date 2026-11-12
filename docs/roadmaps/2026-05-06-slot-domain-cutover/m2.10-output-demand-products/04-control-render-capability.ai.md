# Phase 3: Control Render Capability

## Scope Of Phase

Add engine support for asking a `ControlProduct` owner to render into an
output-owned target.

In scope:

- Add control render request/target/layout engine types.
- Add optional `ControlNode` capability.
- Add node-call/session dispatch for control rendering.
- Add `TickContext` helper for rendering control products.

Out of scope:

- Fixture implementation of `ControlNode`.
- Output demand-root behavior.
- Protocol serialization.

## Code Organization Reminders

- Put control product runtime concepts under `lpc-engine/src/control_product/`.
- Keep `ControlProduct` itself in `lpc-model`.
- Keep capability traits in `lpc-engine/src/node/`.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests.
- If blocked, stop and report.

## Implementation Details

Relevant files:

- `lp-core/lpc-engine/src/control_product/mod.rs`
- `lp-core/lpc-engine/src/control_product/control_render_request.rs`
- `lp-core/lpc-engine/src/control_product/control_render_target.rs`
- `lp-core/lpc-engine/src/control_product/control_layout.rs`
- `lp-core/lpc-engine/src/node/control_node.rs`
- `lp-core/lpc-engine/src/node/node_runtime.rs`
- `lp-core/lpc-engine/src/node/node_call.rs`
- `lp-core/lpc-engine/src/node/contexts.rs`
- `lp-core/lpc-engine/src/resolver/tick_resolver.rs`
- `lp-core/lpc-engine/src/engine/engine.rs`

Expected shape:

```rust
pub enum ControlSampleFormat {
    Unorm16,
}

pub struct ControlRenderRequest {
    pub extent: ControlExtent,
    pub sample_format: ControlSampleFormat,
}

pub struct ControlRenderTarget<'a> {
    pub extent: ControlExtent,
    pub sample_format: ControlSampleFormat,
    pub samples: &'a mut [u16],
}
```

`ControlNode` should render into the provided target and return layout metadata.
Do not allocate a duplicate full frame in the normal path.

## Validate

```bash
cargo test -p lpc-engine
```
