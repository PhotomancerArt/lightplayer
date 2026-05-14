# Phase 1: Docs Vocabulary

## Scope Of Phase

Update durable slot design docs to separate registered shapes, path roots, and
runtime slot objects.

In scope:

- `docs/design/slots/overview.md`
- `docs/design/slots/serialization.md`
- `docs/design/slots/values.md`
- Any nearby roadmap notes that directly contradict the new vocabulary.

Out of scope:

- Code changes.
- API renames.
- New serialization behavior.

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

Expected doc changes:

- Replace the `Slot Roots` concept section with:
  - `Registered Shapes`
  - `Path Roots`
  - `Runtime Slot Objects`
- Define `SlotAccess` as the runtime slot object trait.
- Explain that the engine/wire/storage layer may choose runtime object roots,
  but the shape system does not decide top-levelness.
- Explain that `SlotShape` is the schema node; do not rename to schema in this
  plan.
- Update design rules:
  - "Persisted domain concepts should be slot-modeled..."
  - Avoid "should be slot roots" unless talking about use-site object roots.
- Update serialization docs:
  - `SlotCodecRoot` terminology should become adapter target / codec type in
    prose.
  - Generated adapters target slot-modeled types or registered shapes, not
    necessarily roots.
- Keep `SlotPath::root()` terminology; that refers to the empty path inside the
  current tree.

## Validate

```bash
rg -n "slot roots|Slot Roots|top-level persisted|root ids|slot-root ids|SlotCodecRoot|registered root" docs/design/slots
```

Review remaining matches manually. Some "root" references may remain when they
refer to runtime object roots or path roots.
