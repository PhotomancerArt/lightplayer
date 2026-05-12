# Phase 04: Header Generation

## Scope Of Phase

Generate deterministic GLSL header text from compute shader slot definitions.

In scope:

- Add `shader_header_gen.rs`.
- Generate struct declarations for native `lp::fluid::Emitter`.
- Generate consumed and produced declarations for supported scalar/native/map
  slot defs.
- Lower map slots with `mapping = { kind = "sentinel", ... }` to bounded GLSL
  arrays.
- Add header evidence tests.

Out of scope:

- Editing existing GLSL files.
- Parsing `// gen:header` regions.
- Runtime ABI marshalling.

## Code Organization Reminders

- Keep the generator deterministic.
- Return explicit errors for unsupported types.
- Do not silently guess conversions.

## Sub-Agent Reminders

- Do not commit.
- Do not introduce std-only APIs.
- Do not weaken tests to match accidental formatting.

## Implementation Details

Relevant files:

- `lp-core/lpc-model/src/nodes/shader/shader_header_gen.rs`
- `lp-core/lpc-model/src/nodes/shader/mod.rs`
- `lp-core/lpc-model/src/nodes/fluid/fluid_emitter.rs`

The first evidence header should include:

```glsl
struct FluidEmitter { ... };
uniform float time;
out FluidEmitter emitters[4];
```

The exact formatting can be simple, but stable.

## Validate

```bash
cargo fmt --check
cargo test -p lpc-model shader_header
```

