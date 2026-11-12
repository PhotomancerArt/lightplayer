# Phase 4: Remove ModelValue Texture2D Workaround

sub-agent: yes
model: kimi-k2.5
parallel: -

## Scope of phase

Remove the model-side `Texture2D` value/type variants that encoded runtime
texture references as portable model values.

In scope:

- Remove `ModelValue::Texture2D`.
- Remove `ModelType::Texture2D`.
- Update tests and conversions that referenced those variants.
- Keep `LpsValueF32::Texture2D` intact.
- Preserve source-level texture recipes such as `SrcValueSpec::Texture`.

Out of scope:

- Do not remove or change `LpsValueF32::Texture2D`.
- Do not design texture pixel wire transport.
- Do not remove `Kind::Texture` or its struct storage recipe.
- Do not implement real render products.
- Do not migrate legacy runtimes.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place public types and impls near the top; helpers below them.
- Place tests at the bottom of files.
- Keep related functionality grouped together.
- Any temporary code should have a `TODO` comment so it can be found later.

## Sub-agent Reminders

- Do not commit. The plan commits at the end as a single unit.
- Do not expand scope. Stay strictly within this phase.
- Do not suppress warnings or add `#[allow(...)]`; fix warnings.
- Do not disable, skip, or weaken existing tests.
- If blocked or ambiguous, stop and report instead of improvising.
- Report back: files changed, validation run, result, and deviations.

## Implementation Details

Update:

```text
lp-core/lpc-model/src/prop/model_value.rs
lp-core/lpc-model/src/prop/model_type.rs
lp-core/lpc-engine/src/wire_bridge/lps_value_to_model_value.rs
lp-core/lpc-engine/src/wire_bridge/model_type_to_lps_type.rs
lp-core/lpc-engine/src/resolver/resolver.rs
```

Remove these variants:

```rust
ModelValue::Texture2D { ptr, width, height, row_stride }
ModelType::Texture2D
```

Important distinction:

- `ModelValue` / `ModelType` should not represent runtime texture references.
- `Kind::Texture` may continue to map to a struct storage recipe in
  `kind.rs`.
- `SrcValueSpec::Texture(SrcTextureSpec)` remains the author/source texture
  recipe.
- `LpsValueF32::Texture2D` remains out of scope and should not be removed.

Update `lps_value_f32_to_model_value`:

- Remove the `LpsValueF32::Texture2D` arm mapping to `ModelValue::Texture2D`.
- If exhaustive matching requires handling `LpsValueF32::Texture2D`, introduce
  a small error-returning conversion instead of the current infallible function,
  only if tractable.
- Prefer the smaller local change if possible: if callers only use this for
  non-texture values today, update tests and document that texture conversion is
  no longer represented by `ModelValue`.

If changing the function signature would cause broad churn, stop and report the
blocker rather than improvising a large conversion redesign.

Update `model_type_to_lps_type`:

- Remove `ModelType::Texture2D => LpsType::Texture2D`.
- Keep struct-based texture storage from `Kind::Texture.storage()` working if
  tests depend on it.

Update tests:

- Remove model value/type texture roundtrip tests.
- Replace them, if useful, with assertions that `Kind::Texture.storage()` remains
  a struct recipe and that `SrcValueSpec::Texture` still materializes through
  source-level compatibility tests.

## Validate

Run:

```bash
cargo test -p lpc-model -p lpc-source -p lpc-engine
```
