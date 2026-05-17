# Phase 1: Shared Helper Shape

## Scope Of Phase

Add small shared helper APIs that make generated code compact without hiding
important control flow.

In scope:

- Inspect M1.1 manual helpers and identify repeated map/option/record patterns.
- Add only the helpers needed by the first generated slice.
- Add focused `lpc-model slot_codec` tests for new helpers.

Out of scope:

- Code generation.
- Changing real model serialization behavior.
- Large generic frameworks.

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

- `lp-core/lpc-model/src/slot_codec/slot_reader.rs`
- `lp-core/lpc-model/src/slot_codec/slot_json_writer.rs`
- `lp-core/lpc-model/src/slot_codec/mod.rs`
- `lp-core/lpc-slot-mockup/src/tests/manual_shape_codec.rs`

Candidate helpers:

- `ValueReader::string_key_map(read_value)` returning `BTreeMap<String, T>`.
- `ValueReader::u32_key_map(read_value)` returning `BTreeMap<u32, T>`.
- writer-side helpers only if they materially reduce generated code in phase 2.

Constraints:

- Helpers must remain `no_std + alloc`.
- Avoid introducing trait objects unless there is a clear code-size reason.
- Keep generic nesting shallow.

## Validate

```bash
cargo test -p lpc-model slot_codec
cargo check -p lpc-model --no-default-features
```
