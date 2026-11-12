# Phase 4: Shrink Template Surface

## Scope Of Phase

Remove obsolete hardcoded root sections and leave only intentional generated
fixtures and specialized helper sections.

In scope:

- Delete root-specific hardcoded source-root readers/writers that are now
  rendered from the SlotCodec model.
- Keep or reorganize explicit helper sections.
- Inspect generated output for readability and size.
- Keep generated test-bundle code if it remains useful as a fixture.

Out of scope:

- Generating all helper functions from metadata.
- Removing the representative generated test bundle.
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

Expected changes:

- Remove or significantly shrink hardcoded sections such as:
  - `MOCKUP_SLOT_CODEC_REAL_PROJECT_DEF`
  - any root-specific text now superseded by SlotCodec renderers
- Ensure `render_mockup_slot_codec()` reads like:
  - imports/types
  - generated test bundle fixture
  - generated source-root adapters from the SlotCodec model
  - explicit specialized helpers
- If the generator has become hard to navigate, split codec-specific code out
  of `lib.rs` into one or two focused files. Keep public API unchanged.

Inspection:

- Locate generated file:

```bash
find target/debug/build/lpc-slot-mockup-* -path '*/out/generated_slot_codec.rs' -print
```

- Inspect line count and representative sections.

## Validate

```bash
cargo test -p lpc-slot-codegen
cargo test -p lpc-slot-mockup generated_shape_codec
cargo check -p lpc-slot-mockup
```
