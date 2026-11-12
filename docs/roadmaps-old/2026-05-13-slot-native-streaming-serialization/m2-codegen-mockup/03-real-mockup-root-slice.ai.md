# Phase 3: Real Mockup Root Slice

## Scope Of Phase

Apply the generated codec shape to one real mockup source root.

In scope:

- Choose one relatively small real mockup root, likely `ProjectDef` or
  `OutputDef`.
- Generate or derive slot-native read/write support for that root.
- Add tests that compare generated behavior with existing authored TOML/Serde
  or manual expectations.
- Document any private-field/default-policy friction.

Out of scope:

- Generating all real roots.
- Production adoption.
- Removing Serde derives.

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

- `lp-core/lpc-slot-mockup/src/source/project_def.rs`
- `lp-core/lpc-slot-mockup/src/source/output_def.rs`
- `lp-core/lpc-slot-mockup/src/tests/authored_serde.rs`
- generated module from phase 2

Decision point:

- If real structs are too private for generated construction, record whether M2
  should add generated constructors, generated `Default + mutate` readers, or a
  trait-based field setter approach.

## Validate

```bash
cargo test -p lpc-slot-mockup generated_shape_codec
cargo test -p lpc-slot-mockup authored_serde
```
