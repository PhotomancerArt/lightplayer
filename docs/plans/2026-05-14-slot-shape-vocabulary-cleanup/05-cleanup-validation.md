# Phase 5: Cleanup And Validation

## Scope Of Phase

Finish the vocabulary cleanup, remove stale wording where practical, and run
focused validation.

In scope:

- Search for stale "root" terminology in the touched shape-system areas.
- Update tests or docs that still imply the registry owns top-level objects.
- Write `summary.md` for this plan.
- Run final validation commands.

Out of scope:

- Full CI unless explicitly requested.
- Keeping compatibility methods/attributes if internal usage can be fully
  migrated.
- Redesigning sync/runtime object maps.

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

Run and manually inspect:

```bash
rg -n "slot root|Slot Root|shape root|registered root|root shape|root id|register_root|ensure_root|replace_root|unregister_root|SlotCodecRoot|StaticSlotRoot|slot\\(root" lp-core/lpc-model/src/slot lp-core/lpc-slot-macros/src lp-core/lpc-slot-codegen/src lp-core/lpc-slot-mockup/src docs/design/slots
```

Expected remaining uses:

- `SlotPath::root()` and docs/tests about the empty path.
- `SlotAccessor::root()` and related docs if it clearly means path root.
- `#[slot(root)]` compatibility docs only if the attribute must remain as an
  alias.
- Runtime/wire/storage object-root terminology where the use site truly owns
  the addressable object set.

Add `summary.md` with:

- What was renamed.
- What compatibility aliases remain.
- Any decisions about future schema naming.

## Validate

```bash
cargo fmt
cargo test -p lpc-model slot_shape_registry
cargo test -p lpc-model slot_accessor
cargo test -p lpc-model slot_lookup
cargo test -p lpc-slot-codegen
cargo test -p lpc-slot-mockup generated_shape_codec
cargo test -p lpc-slot-mockup shape_codegen
cargo check -p lpc-model --no-default-features
cargo check -p lpc-wire --no-default-features
cargo check -p lpc-slot-mockup
```
