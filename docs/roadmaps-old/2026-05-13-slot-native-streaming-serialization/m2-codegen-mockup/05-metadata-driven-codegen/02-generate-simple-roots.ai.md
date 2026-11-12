# Phase 2: Generate Simple Roots

## Scope Of Phase

Replace hardcoded generated code for the simple source roots with SlotCodec renderers.

In scope:

- Generate root readers/writers from SlotCodec metadata for:
  - `ProjectDef`
  - `TextureDef`
  - `OutputDef`
- Keep specialized helpers explicit.
- Preserve all existing generated shape codec tests.

Out of scope:

- `FixtureDef` and `ShaderDef`.
- Full enum/helper inference.
- Production adoption.

## Code Organization Reminders

- Prefer granular files with one main concept per file if needed.
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
- `lp-core/lpc-slot-mockup/src/tests/generated_shape_codec.rs`

Expected changes:

- Add renderer functions for root read wrappers:
  - `read_<stem>_json`
  - `read_<stem>_toml`
  - `read_<stem>`
- Add renderer functions for root JSON writers:
  - `write_<stem>_json`
- Generate `ProjectDef`, `TextureDef`, and `OutputDef` root adapters from the
  metadata table.
- Remove the corresponding hardcoded text from
  `MOCKUP_SLOT_CODEC_REAL_PROJECT_DEF` or equivalent sections.
- Keep helper functions like `read_dim2u`, `write_dim2u`, and
  `read_output_driver_options` explicit.

Edge cases:

- `ProjectDef.nodes` uses `ValueReader::string_key_map(read_project_invocation)`.
- `TextureDef.size` uses `read_dim2u`.
- `OutputDef.options` uses default-instance overlay behavior.
- `bindings` should be read-discarded or omitted according to the current
  working behavior.

Tests:

- Existing tests in `generated_shape_codec.rs` must continue to pass unchanged
  unless they need import reordering from generated function names.

## Validate

```bash
cargo test -p lpc-slot-mockup generated_shape_codec
cargo test -p lpc-slot-codegen
```
