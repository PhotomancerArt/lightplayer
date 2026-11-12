# Phase 4: Replace Serde Tests

> Superseded by the measured M4 policy. Keep Serde tests when they cover
> supported protocol/tooling behavior; replace only tests for paths that have
> intentionally moved to SlotCodec.

## Scope Of Phase

Replace or delete `lpc-model` tests that only exercise Serde behavior.

In scope:

- Replace important serde JSON/TOML round trips with SlotCodec, metadata codec,
  `SlotValue`, parser/display, or direct domain tests.
- Delete tests whose only value was proving old serde syntax.
- Remove `serde_json` usage from `lpc-model` tests.
- Remove `toml::from_str/to_string` usage where it depended on Serde. Keep
  TOML parsing to `toml::Value` for SlotCodec tests.

Out of scope:

- Adding broad new behavioral coverage unrelated to serde removal.
- Changing tests in other crates unless required by compile errors.

## Code Organization Reminders

- Keep tests at the bottom of the file.
- Name replacement tests for the behavior they now prove, not for the old serde
  path.
- Prefer focused helper functions over repeating registry setup.
- Do not weaken assertions just to keep a test green.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Search commands:

```bash
rg -n "serde_json|toml::from_str|toml::to_string|serde_" lp-core/lpc-model/src lp-core/lpc-model/tests
```

Replacement patterns:

- For authored node/project TOML:
  - use `NodeDef::read_toml`
  - use `NodeDef::write_toml`
  - use `SlotShapeRegistry::read_slot_toml`
  - use `SlotShapeRegistry::write_slot_toml`
- For wire-ish slot payloads:
  - use `SlotShapeRegistry::read_slot_json`
  - use `SlotShapeRegistry::write_slot_json`
- For semantic leaves:
  - use `ToLpValue` / `FromLpValue`
  - use `slot_codec::read_lp_value` / `write_lp_value` when syntax matters
- For slot metadata:
  - use Phase 2 metadata codec helpers
- For string reference types:
  - use `parse` and `to_string`

Tests that likely need attention:

- `slot/value_slot.rs`
- `slot/slot_data.rs`
- `slot/slot_shape.rs`
- `slot/slot_path.rs`
- `slot/slot_name.rs`
- `binding/*`
- `node/node_invocation.rs`
- `value/lp_value.rs`
- `value/lp_type.rs`
- `value/legacy_kind.rs`
- `value/constraint.rs`
- `nodes/**`
- `slots/mod.rs`

## Validate

```bash
cargo fmt -p lpc-model
cargo test -p lpc-model
rg -n "serde_json|serde_|toml::from_str|toml::to_string" lp-core/lpc-model/src lp-core/lpc-model/tests
```

The final search may still find `toml::from_str::<toml::Value>` in SlotCodec
tests or node TOML entry points. That is allowed if it does not depend on Serde.
