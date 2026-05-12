# M2.9 Summary: Runtime Def View Cutover

## Implemented

- Added generated slot views for `ShaderDef`, `FixtureDef`, and `OutputDef`, matching the existing `TextureDefView` path.
- Removed copied `ShaderDef` ownership from `ShaderNode`; shader compile options are now read through resolver-backed slot accessors and invalidate the cached shader when changed.
- Removed copied fixture scalar config from `FixtureNode`; render size, color order, brightness, and gamma correction are now resolved through consumed def slots.
- Extended `RuntimeProduct` with `ModelValue(LpValue)` so resolver literals and authored def fallback can carry rich non-shader values such as strings and structs.
- Extended `SlotAccessor` with option payload access via the conventional `.some` field.
- Updated runtime loader and manual tests for the new constructor shapes.
- Updated the cutover todo list with completed work and the next generated-option-view follow-up.

## Intentional Boundaries

- Fixture `mapping` and output sink setup remain loader-owned for now because mapping changes affect resource allocation and cached precomputed data.
- Output runtime behavior is not converted to read `OutputDefView` yet; service registration still happens outside node tick.
- Generated views currently expose option fields at the option root. Fixture uses a narrow hand-authored `.some` accessor until codegen grows option payload helpers.

## Validation

- `cargo fmt --check`
- `cargo test -p lpc-model`
- `cargo test -p lpc-slot-codegen`
- `cargo test -p lpc-engine`
- `cargo check -p lpc-model --features schema-gen`
- `cargo clippy -p lpc-engine -p lpc-model -p lpc-slot-codegen -p lpc-slot-macros --all-targets -- -D warnings`
