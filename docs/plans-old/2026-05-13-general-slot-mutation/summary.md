# Implementation Summary

## What was built

- Changed slot policy to default to writable persisted data.
- Added `SlotRecord` derive support for container-level policy overrides and field-level policy overrides.
- Removed conservative read-only opt-outs from authored node-definition fields like `bindings` and project `nodes`.
- Added mutable slot access traits and lookup helpers so typed authored data can be updated generically through slot paths.
- Replaced the clock-specific engine mutation path with generic `SetValue` handling for writable value leaves under `node.<id>.def`.
- Kept `node.<id>.state` roots hardcoded as non-mutable for this phase.
- Added regression coverage for derive policy defaults, writable authored output fields, generic engine mutation, and client/view mutation behavior.

## Decisions for future reference

#### Mutability is the default

- **Decision:** `SlotPolicy::default()` is writable persisted.
- **Why:** LightPlayer's primary authoring surface is the UI, so authored data should be mutable unless there is a semantic reason it is not authorable.
- **Rejected alternatives:** Requiring every authored def to opt into mutability with `default_policy = "writable_persisted"`.
- **Revisit when:** A slot domain needs a different semantic default and can express that at its container boundary.

#### Runtime state remains out of scope

- **Decision:** Runtime state records explicitly use read-only transient policy, and generic mutation still accepts only `node.<id>.def` roots in this phase.
- **Why:** Runtime state is visible but not authored configuration.
- **Rejected alternatives:** Treating unsupported authored mutation capability as read-only policy.
- **Revisit when:** Node runtime state becomes a supported editable API with explicit guardrails.

#### Generic mutation writes through typed model access

- **Decision:** Added mutable slot access traits over existing typed node-def storage instead of introducing a mutable shadow `SlotData` tree.
- **Why:** This keeps the authored data model single-sourced and removes the need for more node-kind-specific mutation code.
- **Rejected alternatives:** Repeating per-node path mutation switches or maintaining a second dynamic mutation store.
- **Revisit when:** Container mutation or broader runtime editing requires richer mutation primitives than value-leaf replacement.
