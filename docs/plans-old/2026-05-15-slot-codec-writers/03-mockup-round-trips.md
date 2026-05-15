# Phase 3: Mockup Round Trips

## Scope Of Phase

Prove the new generic writers against the mockup domain.

In scope:

- extend `dynamic_slot_codec.rs` to cover JSON writes
- extend `dynamic_slot_codec.rs` to cover TOML writes
- round trip static mockup objects through registry write/read APIs
- round trip a registered dynamic shape where possible
- mark old writer-dependent tests as ready for removal in the later cleanup

Out of scope:

- deleting `generated_shape_codec.rs`
- deleting `manual_shape_codec.rs`
- deleting `storage_codec.rs`
- changing real engine or wire callers

## Code Organization Reminders

- Keep mockup tests focused on the generic registry path.
- Do not add new mockup-specific codec helpers unless they are tiny assertion
  helpers.
- Prefer downcasting static registry reads back to concrete mockup types.
- Keep helper functions below test functions.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-slot-mockup/src/tests/dynamic_slot_codec.rs`
- `lp-core/lpc-slot-mockup/src/tests/storage_codec.rs`
- `lp-core/lpc-slot-mockup/src/tests/generated_shape_codec.rs`

Add tests to `dynamic_slot_codec.rs`:

- `dynamic_slot_codec_writes_project_json_through_registry`
- `dynamic_slot_codec_round_trips_project_json_through_registry`
- `dynamic_slot_codec_writes_project_toml_through_registry`
- `dynamic_slot_codec_round_trips_project_toml_through_registry`
- fixture enum payload write/read round trip
- dynamic registered `ShaderNode` shape write/read if the dynamic object access
  path supports it cleanly

Use APIs from the previous phases:

```rust
let json = registry.write_slot_json(&project, Vec::new()).unwrap();
let read = registry
    .read_slot_json(ProjectDef::SHAPE_ID, core::str::from_utf8(&json).unwrap())
    .unwrap()
    .into_any()
    .downcast::<ProjectDef>()
    .unwrap();

let toml = registry.write_slot_toml(&project).unwrap();
let read = registry
    .read_slot_toml(ProjectDef::SHAPE_ID, &toml)
    .unwrap()
    .into_any()
    .downcast::<ProjectDef>()
    .unwrap();
```

For now, do not delete old tests. Instead, make sure the new test coverage is
strong enough that the cleanup plan can delete them next.

## Validate

```bash
cargo fmt -p lpc-model -p lpc-slot-mockup --check
cargo test -p lpc-model dynamic_slot_writer
cargo test -p lpc-slot-mockup dynamic_slot_codec
```
