# Phase 1: Shape Coverage Fixture

## Scope Of Phase

Create the test-only mock source bundle and coverage matrix for M1.1.

In scope:

- Add a new test module such as
  `lp-core/lpc-slot-mockup/src/tests/manual_shape_codec.rs`.
- Register the module in `lp-core/lpc-slot-mockup/src/tests/mod.rs`.
- Define test-only fixture data if the real mockup source structs are not
  ergonomic enough to instantiate/compare directly.
- Ensure the fixture covers roots, nested records, maps, options, enums,
  arrays, scalar leaves, bindings, and node invocations.

Out of scope:

- Implementing full reader/writer functions.
- Codegen.
- Production model changes unless a tiny accessor/constructor is clearly needed
  for test ergonomics.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- Keep related functionality grouped together.
- Put helpers lower in the file when that improves readability.
- Mark any temporary code with a clear `TODO`.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-slot-mockup/src/tests/manual_shape_codec.rs`
- `lp-core/lpc-slot-mockup/src/tests/mod.rs`
- `lp-core/lpc-slot-mockup/src/source/*.rs`
- `lp-core/lpc-model/src/binding/*.rs`

Expected fixture shape:

- `ManualSourceBundle` or similar with fields:
  - project root
  - output node def
  - texture node def
  - shader node def
  - fixture node def
- Project root should include a string-key map of node invocations.
- Node definition wrapper should include at least `OutputDef`, `TextureDef`,
  `ShaderDef`, and `FixtureDef` variants.
- Fixture mapping should include at least:
  - `PathPoints` with numeric-key `paths`
  - nested `RingArray` path spec with numeric-key `ring_lamp_counts`
  - one unit variant such as `Disabled` in a second fixture or alternate sample
- Output options should cover `Option::Some(record)`.
- Shader params or fixture brightness should cover `Option::None`.
- Bindings should cover a map of binding definitions and endpoint strings.

Tests to add in this phase:

- A small coverage assertion test that constructs the fixture and checks it has
  non-empty maps/options/enums.

## Validate

```bash
cargo test -p lpc-slot-mockup manual_shape_codec
```
