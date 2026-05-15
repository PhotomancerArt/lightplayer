# Phase 5: Remove From-Codec Scaffolding

## Scope Of Phase

Delete codec-only constructors from the mockup domain after generated code no
longer needs them.

In scope:

- remove mockup `from_codec` and `*_from_codec` functions
- keep normal human/domain constructors
- update tests if they referenced codec-only constructors
- add a search/assertion that no generated or mockup source code still uses
  `from_codec`

Out of scope:

- renaming unrelated constructors
- changing production domain constructors
- broader API polish

## Code Organization Reminders

- Domain source files should expose constructors that make sense to humans.
- Codec-only wrapping belongs in generated code or shared codec helpers.
- Keep tests at the bottom of files.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-slot-mockup/src/source/output_def.rs`
- `lp-core/lpc-slot-mockup/src/source/texture_def.rs`
- `lp-core/lpc-slot-mockup/src/source/fixture_def.rs`
- `lp-core/lpc-slot-mockup/src/source/shader_def.rs`
- `lp-core/lpc-slot-mockup/src/source/mapping.rs`
- `lp-core/lpc-slot-codegen/src/lib.rs`

Remove or replace:

- `OutputDef::from_codec`
- `OutputDriverOptionsConfig::from_codec`
- `TextureDef::from_codec`
- `FixtureDef::from_codec`
- `ShaderDef::from_codec`
- `MappingConfig::square_from_codec`

Keep constructors such as:

- `new`
- `default`
- `path_points`
- `ring_array`
- test/data convenience constructors that are not codec-specific

Tests to add/update:

- Add or update a codegen test that `render_mockup_slot_codec()` does not
  contain `from_codec`.
- Run a repository search:

```bash
rg "from_codec|_from_codec" lp-core/lpc-slot-mockup/src lp-core/lpc-slot-codegen/src
```

The search should be empty or should only show intentionally renamed non-codec
concepts, if any.

## Validate

```bash
cargo test -p lpc-slot-codegen
cargo test -p lpc-slot-mockup
```

