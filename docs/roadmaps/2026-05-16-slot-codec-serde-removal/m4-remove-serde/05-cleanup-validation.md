# Phase 5: Cleanup And Validation

## Scope Of Phase

Finish dependency removal, update docs, and run final validation.

In scope:

- Remove `serde`, `serde_json`, `schemars`, and `toml/serde` from
  `lpc-model/Cargo.toml`.
- Remove stale docs that describe serde-backed model behavior.
- Search for leftover serde/schema-gen strings.
- Run final targeted validation.
- Fix formatting, warnings, and residual compile errors.

Out of scope:

- Removing Serde from crates other than `lpc-model` unless required by
  `lpc-model` API changes.
- New feature work.
- Schema generation replacement.

## Code Organization Reminders

- Do not leave commented-out serde derives or TODOs.
- Keep docs precise about the post-M4 state.
- Avoid broad rewrites outside the dependency boundary.
- Keep final search results intentional and documented if anything remains.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Dependency cleanup:

- `lp-core/lpc-model/Cargo.toml`
  - remove `serde`
  - remove `serde_json`
  - remove `schemars`
  - remove `schema-gen`
  - remove `toml/serde` feature

Docs to review:

- `docs/design/slots/overview.md`
- `docs/design/slots/serialization.md`
- `docs/design/slots/values.md`
- `docs/roadmaps/2026-05-16-slot-codec-serde-removal/overview.md`
- `docs/roadmaps/2026-05-16-slot-codec-serde-removal/decisions.md`
- `docs/roadmaps/2026-05-16-slot-codec-serde-removal/m4-remove-serde.md`

Final search gates:

```bash
rg -n "serde|serde_json|Serialize|Deserialize|schemars|schema-gen" lp-core/lpc-model
rg -n "lpc-model/schema-gen" .
```

Acceptable exceptions:

- Mentions in archived roadmap/plan docs may remain if they document history.
- Mentions in other crates are outside scope unless they reference removed
  `lpc-model` features.

## Validate

```bash
cargo fmt -p lpc-model -p lpc-source -p lpc-wire -p lpc-view -p lpc-shared -p lpc-engine
cargo check -p lpc-model
cargo test -p lpc-model
cargo test -p lpc-slot-mockup
cargo test -p lpc-shared project::builder
cargo test -p lpc-engine project_loader
cargo check -p lpc-source
cargo check -p lpc-wire
cargo check -p lpc-view
cargo check -p lpc-shared
git diff --check
```
