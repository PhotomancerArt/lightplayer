# ModelValue Resource

## Scope Of Phase

Add portable resource references to the existing model value/type layer.

In scope:

- Add `ModelValue::Resource(ResourceRef)`.
- Add matching `ModelType::Resource`.
- Update serde/schema derives and tests.
- Update any model/type matching helpers in `lpc-model`.

Out of scope:

- Replacing `RuntimeProduct`.
- Reworking wire resource payload APIs.
- Renaming `ModelValue`.
- Slot data structures.

## Code Organization Reminders

- Keep changes focused in `lp-core/lpc-model/src/prop/` and
  `lp-core/lpc-model/src/resource.rs` only if a helper is needed.
- Tests stay at the bottom of Rust files.
- Rustdocs should state that `ResourceRef` is a portable reference; payload
  bytes are fetched separately.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-model/src/prop/model_value.rs`
- `lp-core/lpc-model/src/prop/model_type.rs`
- `lp-core/lpc-model/src/resource.rs`
- `lp-core/lpc-model/src/prop/mod.rs`
- `lp-core/lpc-model/src/lib.rs`

Expected changes:

- Import `ResourceRef` where needed.
- Add `ModelValue::Resource(ResourceRef)`.
- Add `ModelType::Resource`.
- Add serde round-trip tests for `ModelValue::Resource`.
- Add type round-trip tests for `ModelType::Resource`.
- If there are functions matching all model value/type variants, update them.

Constraints:

- Keep `ResourceRef` as a reference only. Do not add payload bytes to
  `ModelValue`.
- Do not rename `RuntimeProduct`; that is outside M1.
- Maintain `no_std + alloc` compatibility.

## Validate

```bash
cargo test -p lpc-model
```

