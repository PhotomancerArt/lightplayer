# Phase 3: Generated Record Construction

## Scope Of Phase

Change generated mockup record readers to construct record literals directly
instead of calling codec-only domain constructors.

In scope:

- generated readers for discovered mockup records
- direct construction of source records and nested records
- preserve current tests and data shapes

Out of scope:

- deleting all `from_codec` functions
- enum body generation cleanup
- production adoption

## Code Organization Reminders

- Generated code should be boring and readable.
- Prefer shared helper calls over large repeated code blocks.
- Avoid adding generated code that is substantially more verbose than the old
  handwritten shape without a clear reason.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-slot-codegen/src/lib.rs`
- generated output included by:
  - `lp-core/lpc-slot-mockup/src/lib.rs`
- mockup records:
  - `source/output_def.rs`
  - `source/texture_def.rs`
  - `source/fixture_def.rs`
  - `source/shader_def.rs`
  - `source/project_def.rs`

Expected changes:

- Generated `read_*` functions should end with record literals, not
  `Type::from_codec(...)`.
- For records with defaults, generate:

```rust
let defaults = Type::default();
let mut field = defaults.field.clone();
```

where the field type is cloneable.

- For fields not present in a codec surface, make the generated policy visible:

```rust
let bindings = BindingDefs::default();
let sampling = ValueSlot::new(FixtureSamplingConfig::TextureArea);
```

- Keep current top-level discriminator behavior:

```rust
object.expect_discriminator("kind", &[Type::KIND])?
```

Tests to add/update:

- Existing generated mockup JSON/TOML round-trip tests should pass.
- Add one test or codegen assertion that generated code no longer contains
  `::from_codec(` for record construction.

## Validate

```bash
cargo test -p lpc-slot-codegen
cargo test -p lpc-slot-mockup generated_shape_codec
cargo test -p lpc-slot-mockup storage_codec
```
