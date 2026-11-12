# Phase 3: Example And Serde Evidence

## Scope Of Phase

Update the canonical example and add evidence tests that show the authored
binding language working in real TOML.

In scope:

- Update `examples/basic` to use bus-first bindings.
- Add direct node-slot binding examples in tests.
- Add focused TOML round-trip tests for the canonical node files.
- Keep example updates source-level; runtime flow may remain partially
  transitional until M2.4.

Out of scope:

- Full runtime shader/texture/fixture behavior changes.
- UI or wire sync work.
- Broad example migration beyond what tests require.

## Code Organization Reminders

- Prefer fixture/helper files only when they make tests easier to scan.
- Keep test helper functions below tests.
- Do not hide large expected TOML strings in generic `mod.rs` files.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `examples/basic/project.toml`
- `examples/basic/shader.toml`
- `examples/basic/texture.toml`
- `examples/basic/fixture.toml`
- source crate tests that parse examples, likely under `lp-core/lpc-source`
- any generated TOML evidence tests from prior M1.2/M2 work

Canonical intended shape:

```toml
# shader.toml
kind = "shader"
glsl_path = "shader.glsl"
render_order = 0

[bindings.output]
target = "bus#visual.out"
```

```toml
# texture.toml
kind = "texture"

[size]
width = 16
height = 16

[bindings.input]
source = "bus#visual.out"
```

Fixture binding depends on how far Phase 2 moved output/texture references. The
likely target is:

```toml
[bindings.input]
source = "..texture#output"
```

or, if bus-first texture output is also shown:

```toml
[bindings.input]
source = "bus#texture.out"
```

The important evidence is that both bus and node-slot endpoint syntaxes parse
into semantic Rust values.

Tests to add/update:

- `examples/basic` node TOML parses into source defs.
- Shader binding `output.target` parses as `BindingEndpoint::Bus`.
- Texture binding `input.source` parses as `BindingEndpoint::Bus`.
- Direct node-slot test parses `..shader#output` as `BindingEndpoint::Node`.
- Round-trip serialization does not fall back to old `texture_loc` shape.

## Validate

Run:

```bash
cargo fmt --package lpc-source --package lpc-model
cargo test -p lpc-model
cargo test -p lpc-source
```
