# Phase 5: Output Consumes Control

## Scope Of Phase

Make output nodes the runtime demand roots that consume `ControlProduct` and own
control buffers.

In scope:

- Add `OutputDef.bindings` with `input` convention.
- Make `OutputNode::tick` resolve `input` as `ControlProduct`.
- Output decides actual `ControlExtent`.
- Output owns/sizes the runtime output buffer as `unorm16` samples.
- Output asks the product owner to render into that buffer.
- Project loader adds output nodes as demand roots.
- Remove `FixtureDef.output_loc` and loader special-casing around output sinks.

Out of scope:

- Moving provider IO out of `RuntimeServices`.
- Rich protocol-specific extent mapping.
- Client UI work.

## Code Organization Reminders

- Keep loader changes localized to project runtime loading.
- Remove obsolete push-path code instead of leaving speculative compatibility.
- Use positive, domain-specific names for config booleans and fields.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests.
- If blocked, stop and report.

## Implementation Details

Relevant files:

- `lp-core/lpc-model/src/nodes/output/output_def.rs`
- `lp-core/lpc-model/src/nodes/fixture/fixture_def.rs`
- `lp-core/lpc-engine/src/nodes/output/output_node.rs`
- `lp-core/lpc-engine/src/project_runtime/project_loader.rs`
- `lp-core/lpc-engine/src/project_runtime/runtime_services.rs`
- `examples/basic/output.toml`
- `examples/basic/fixture.toml`

Expected behavior:

- Output nodes are demand roots for project-loaded runtimes.
- Fixture nodes are pulled through output input resolution.
- Output buffers remain registered with `RuntimeServices` for flushing.
- Existing provider write path still receives `u16` payloads.

## Validate

```bash
cargo test -p lpc-model
cargo test -p lpc-engine
```
