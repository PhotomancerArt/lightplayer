## Scope of Phase

Add the M2 runnable validation slice using dummy core nodes and a test-only
builder.

The slice should be semantically shaped like the legacy shader -> fixture ->
output flow, but must use the new `Node`, `Engine`, `BindingRegistry`,
`Resolver`, and `ResolveSession` path. The goal is concise tests that prove the
engine contract without 80 lines of setup per case.

Out of scope:

- Do not adapt concrete legacy `ShaderRuntime`, `FixtureRuntime`, or
  `OutputRuntime`.
- Do not add a text/filetest DSL yet.
- Do not add UI/wire binding sync.
- Do not add render products.

Suggested sub-agent model: `kimi-k2.5`.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place public items and entry points near the top, helpers below them, and
  `#[cfg(test)] mod tests` at the bottom of Rust files.
- Keep related functionality grouped together.
- Test helpers live below the tests that use them.
- Any temporary code must have a TODO comment so it can be found later.

## Sub-agent Reminders

- Do not commit. The plan commits at the end as a single unit.
- Do not expand scope. Stay strictly within "Scope of Phase".
- Do not suppress warnings or `#[allow(...)]` problems away; fix them.
- Do not disable, skip, or weaken existing tests to make the build pass.
- If something blocks completion, stop and report back rather than improvising.
- Report back what changed, what was validated, and any deviations from this
  phase plan.

## Implementation Details

Add test support close to the engine module:

```text
lp-core/lpc-engine/src/engine/test_support.rs
```

This module should be compiled only for tests:

```rust
#[cfg(test)]
pub(crate) mod test_support;
```

Add dummy nodes named around legacy roles:

- `DummyShaderNode`: produces a versioned output and counts how many times it
  was ticked.
- `DummyFixtureNode`: demand root that resolves an input, usually through
  `QueryKey::NodeInput` or `QueryKey::Bus`, and records the consumed value.
- `DummyOutputNode`: records output-like side effects if useful for tests.

The dummy nodes should implement `crate::node::Node` and expose output values
through `RuntimePropAccess`.

Add `EngineTestBuilder` with fluent helpers. Exact API can vary, but aim for
tests that read like:

```rust
let mut engine = EngineTestBuilder::new()
    .shader("shader", output("outputs[0]", 0.75))
    .fixture("fixture")
    .bind_bus("video_out", node_output("shader", "outputs[0]"))
    .bind_input("fixture", "inputs[0]", bus("video_out"))
    .demand_root("fixture")
    .build();
```

Use existing runtime IDs internally. The builder may map string labels to
`NodeId` for readability, but do not introduce those labels into production
types.

Tests to add:

- `fixture_resolves_shader_output_through_bus`
- `producer_runs_once_when_demanded_twice_in_same_frame`
- `bus_selects_highest_priority_binding`
- `equal_priority_bus_bindings_error`
- `recursive_bus_cycle_errors`
- `resolve_trace_records_value_origin_path`
- `binding_registry_versions_are_available_for_debug_list`

Keep tests concise. If a test needs more than roughly 15-20 lines, improve the
builder/helper API rather than expanding boilerplate.

## Validate

Run:

```bash
cargo test -p lpc-engine engine
cargo test -p lpc-engine resolver
cargo test -p lpc-engine binding
```

If all pass, run the broader crate tests:

```bash
cargo test -p lpc-engine
```
