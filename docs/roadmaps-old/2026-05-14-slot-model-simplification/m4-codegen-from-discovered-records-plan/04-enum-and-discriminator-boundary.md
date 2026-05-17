# Phase 4: Enum And Discriminator Boundary

## Scope Of Phase

Make enum/discriminator codec handling explicit and limited, without
reintroducing hidden record field lists.

In scope:

- keep or improve generated helpers for `MappingConfig`, `PathSpec`, and node
  definition wrappers
- ensure enum discriminators produce friendly invalid-value errors
- avoid using enum handling as a back door for record shadow schemas

Out of scope:

- deriving slot enum codec metadata from Rust enums
- supporting nested wrapper enums beyond current mockup needs
- production `NodeDef` adoption

## Code Organization Reminders

- Keep explicit enum handling close to codec generation.
- Use clear names that signal custom enum policy.
- Do not mix enum metadata with discovered record metadata.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-slot-codegen/src/lib.rs`
- `lp-core/lpc-slot-mockup/src/source/mapping.rs`
- `lp-core/lpc-slot-mockup/src/tests/generated_shape_codec.rs`
- `lp-core/lpc-slot-mockup/src/tests/manual_shape_codec.rs`

Expected changes:

- Audit the generated helpers for:
  - `read_mapping_config`
  - `read_mapping_square_body`
  - `read_mapping_path_points_body`
  - `read_path_spec`
  - node-def wrapper readers
- Keep discriminator strings explicit and friendly.
- If `MappingConfig::square_from_codec` is still used, replace it with direct
  variant construction:

```rust
Ok(MappingConfig::Square {
    variant_revision: current_revision(),
    origin: XySlot::new(Xy(origin)),
    size: XySlot::new(Xy(size)),
})
```

- Prefer a helper function in generated code over a domain `from_codec`
  constructor if the code would otherwise be repeated.

Tests to add/update:

- Existing invalid discriminator tests must still pass.
- Add a focused test that `square` or another enum variant can be read without
  a domain `from_codec` helper.

## Validate

```bash
cargo test -p lpc-slot-mockup generated_shape_codec_invalid_discriminator_reports_valid_values
cargo test -p lpc-slot-mockup manual_shape_codec_invalid_discriminator_reports_valid_values
cargo test -p lpc-slot-mockup
```

