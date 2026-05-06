# Phase 5: Convert Complex Mockup Records

## Scope Of Phase

Use the derive for the remaining static mockup records while leaving dynamic and enum-shaped data manual.

In scope:

- Convert:
  - `ProjectDef`
  - `ShaderDef`
  - `CompilerOptions`
  - `ShaderParamDef`
  - `FixtureDef`
  - `FixtureNode`
- Keep manual:
  - `ShaderNode` dynamic params shape/data.
  - `FixtureMapping` enum access.
- Remove duplicated manual field-order impls wherever derive covers them.
- Add or update tests to catch field order/shape mismatches.

Out of scope:

- Enum derive.
- Dynamic shader param derive.
- Real `lpc-source` / `lpc-engine` conversion.

## Code Organization Reminders

- Prefer explicit annotations over clever macro inference.
- Keep dynamic shape behavior in `ShaderNode` hand-authored.
- Do not weaken existing sync/mutation tests.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Watch these cases:

- `ShaderDef::register_shape` currently registers reusable roots:
  - `source.scalar_hint`
  - `source.shader_param_def`
  - `source.shader`
- After derive, reusable roots should still register through each type’s `StaticSlotAccess`.
- `ProjectDef` contains a `SlotMap<String, NodeInvocationDef>`.
- `FixtureDef` contains:
  - value leaves
  - manual enum `FixtureMapping`
  - `SlotOption<ScalarHint>`
- `FixtureNode` contains:
  - `SlotMap<u32, TouchState>`
  - manual enum `FixtureMapping` preview

If the derive needs a small additional field annotation for direct enum access, add it deliberately and document it in macro rustdocs.

## Validate

```bash
cargo test -p lpc-slot-mockup
cargo test -p lpc-slot-mockup -- --nocapture --test-threads=1
cargo test -p lpc-model --features derive
cargo check -p lpc-wire --features schema-gen
git diff --check
```
