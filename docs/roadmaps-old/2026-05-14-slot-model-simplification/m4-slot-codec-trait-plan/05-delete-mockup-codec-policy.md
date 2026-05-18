# Phase 5: Delete Mockup Codec Policy

## Scope Of Phase

Remove the old shadow schema and prove the mockup codec is driven by slot-discovered records plus `SlotCodec` behavior.

In scope:

- delete `mockup_codec_policy()`
- delete old `MockupCodecRecord`, `MockupCodecField`, and constructor-expression machinery
- delete generated helpers made obsolete by `SlotCodec`
- update tests to assert the new generation path

Out of scope:

- broad domain model cleanup
- real loader/message adoption
- optimizing generated code size beyond obvious helper reuse

## Code Organization Reminders

- Keep any remaining surface configuration small, named, and documented.
- If a field needs special behavior, prefer adding metadata or a type-owned codec before adding a policy entry.
- Any exception list should be clearly visible and small enough to review at a glance.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Search for and remove:

```bash
rg "mockup_codec_policy|MockupCodecRecord|MockupCodecField|MockupCodecConstructor|read_dim2u|write_dim2u|read_affine2d|write_affine2d" lp-core/lpc-slot-codegen/src lp-core/lpc-slot-mockup/src -n
```

Some names may still exist in manual tests. The goal is to remove them from generated mockup codec output, not necessarily from the manual comparison test if that test is still intentionally documenting the old hand-written path.

Update codegen tests that currently assert policy contents. Replace them with tests that assert:

- discovered records include representative fields
- generated output contains `impl SlotCodec for ...`
- generated output calls `SlotCodec::read_slot` or `.write_slot(...)`
- generated output does not contain field-specific read/write helper names

Update mockup generated codec tests to prove:

- JSON read works
- TOML read works
- JSON write works
- round-trip works for representative records
- unknown fields error
- invalid discriminator error lists valid values

## Validate

```bash
cargo test -p lpc-slot-codegen
cargo test -p lpc-slot-mockup
rg "mockup_codec_policy|MockupCodecRecord|MockupCodecField|MockupCodecConstructor" lp-core/lpc-slot-codegen/src lp-core/lpc-slot-mockup/src -n
```
