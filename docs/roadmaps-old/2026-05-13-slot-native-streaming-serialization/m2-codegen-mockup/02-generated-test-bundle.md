# Phase 2: Generated Test Bundle

## Scope Of Phase

Generate a codec for a representative test bundle equivalent to the M1.1 manual
fixture.

In scope:

- Add a build-script generation entry point in `lpc-slot-codegen`.
- Generate code into `OUT_DIR` for the mockup crate.
- Add a module in `lpc-slot-mockup` that includes the generated code.
- Add generated-code tests equivalent to M1.1 manual tests.
- Keep generated code compact and helper-driven.

Out of scope:

- Full derive macro support.
- Real production model adoption.
- Replacing the M1.1 manual tests.

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

- `lp-core/lpc-slot-codegen/src/lib.rs`
- `lp-core/lpc-slot-mockup/build.rs`
- `lp-core/lpc-slot-mockup/src/lib.rs`
- `lp-core/lpc-slot-mockup/src/tests/generated_shape_codec.rs`

Suggested generated surface:

- A test-local generated module with bundle structs and codec functions, or
  codec impls for test-local structs if that proves easier.
- Functions similar to:
  - `read_bundle(reader) -> Result<GeneratedSourceBundle, SyntaxError>`
  - `write_bundle_json(bundle) -> Vec<u8>`

Generated code should use shared helpers for maps/arrays/discriminators where
available. It should avoid duplicating large scanner bodies when helper calls
are enough.

Tests:

- JSON round-trip.
- TOML read.
- unknown field error.
- invalid discriminator error.
- missing required field error.

## Validate

```bash
cargo test -p lpc-slot-mockup generated_shape_codec
cargo test -p lpc-slot-mockup manual_shape_codec
```
