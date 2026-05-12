# Phase 4: Generated TOML Evidence Harness

## Scope Of Phase

Add a mockup test harness that generates authored TOML evidence from Rust
source models, parses it back, and verifies slot behavior.

In scope:

- Generate source-like mockup defs from Rust constructors.
- Serialize those defs to TOML.
- Write evidence files under a gitignored generated path.
- Parse the TOML back into mockup defs.
- Register shapes and snapshot/traverse parsed source roots.
- Print enough evidence for easy hand inspection.

Out of scope:

- Hand-maintained fixture files.
- Real `examples/basic` changes.
- Real `lpc-source` conversion.
- Golden/snapshot test infrastructure unless needed for readability.

## Code Organization Reminders

- Keep evidence helpers in test support files, not production source modules.
- Prefer readable helper names over dense generic test plumbing.
- Keep generated outputs under `target/`.
- Do not commit generated evidence files.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-slot-mockup/src/tests/fixture.rs`
- New test file: `lp-core/lpc-slot-mockup/src/tests/authored_serde.rs`
- `lp-core/lpc-slot-mockup/src/tests/mod.rs` or equivalent test module file
- `lp-core/lpc-slot-mockup/Cargo.toml` if test dependencies/features need small
  adjustments.

Generated evidence path:

```text
target/slot-mockup-evidence/source-basic/
```

Expected test flow:

1. `set_current_state_version(FrameId::new(11))`.
2. Build representative source defs:
   - project
   - shader
   - fixture with source-like mapping
   - output
   - texture
3. Serialize each def with `toml::to_string_pretty`.
4. Write each TOML file to the evidence path.
5. Print the evidence path and short TOML snippets or file names.
6. Parse every TOML string back to the same mockup def type.
7. Assert parsed slot fields/container structures carry frame `11`.
8. Register source shapes.
9. Snapshot parsed source roots through real slot access sync.
10. Assert representative slot paths exist, including nested map paths.

Representative paths:

- `project#nodes[shader].artifact`
- `shader#glsl_path`
- `shader#texture_loc`
- `shader#param_defs[exposure].default`
- `fixture#mapping.paths[0].ring_lamp_counts[0]`
- `output#pin`
- `texture#size`

Constraints:

- Evidence files are generated artifacts, not source fixtures.
- `target/` is gitignored; if a different path is used, add a focused
  `.gitignore` entry.
- The test should be useful with `cargo test -p lpc-slot-mockup -- --nocapture`.

## Validate

```bash
cargo fmt --package lpc-slot-mockup
cargo test -p lpc-slot-mockup -- --nocapture
```
