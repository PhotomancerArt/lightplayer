# Phase 5: Cleanup, Validation, and Summary

## Metadata

- **sub-agent:** supervised
- **model:** composer-2
- **parallel:** -

## Scope of Phase

Clean up the whole M3 source migration, run final validation including the
`lp-cli profile` perf-tool path, and write the roadmap plan summary.

In scope:

- Search the live code/config diff for leftover `node.json` references.
- Remove stray temporary files, debug prints, commented-out code, and accidental
  TODOs introduced by this plan.
- Run formatting if needed.
- Run targeted crate tests.
- Run the `lp-cli profile` validation against `examples/perf/fastmath`.
- Write `summary.md` in this plan directory.

Out of scope:

- Do not archive the plan directory; this is a roadmap milestone plan and stays
  under `docs/roadmaps/...`.
- Do not commit.
- Do not fix hard behavioral bugs by expanding scope; report blockers.
- Do not edit archived plans just to remove historical `node.json` mentions.

## Code Organization Reminders

- Keep cleanup edits scoped to this plan's touched areas.
- Prefer removing temporary code over leaving TODOs.
- Keep related functionality grouped together.
- Tests belong at the bottom of Rust source files.

## Sub-Agent Reminders

- Do not commit. The plan commits at the end as a single unit.
- Do not expand scope. Stay strictly within this phase.
- Do not suppress warnings or add `#[allow(...)]` to hide problems.
- Do not disable, skip, or weaken existing tests.
- If validation fails with a non-obvious runtime, compiler, or profile bug,
  stop and report. Do not spin in a debugging loop.
- Report back: cleanup performed, validation run, validation result, remaining
  risks, and any deviations.

## Implementation Details

Read the shared context first:

- `docs/roadmaps/2026-05-01-runtime-core/m3-legacy-source-migration/00-notes.md`
- `docs/roadmaps/2026-05-01-runtime-core/m3-legacy-source-migration/00-design.md`

Search for leftover live `node.json` references. Historical docs under
`docs/plans-old/**`, older roadmap/design docs, and `00-notes.md` references
that explain the migration do not all need to be removed. Live code, examples,
builders, templates, tests, and comments should use `node.toml`.

Suggested searches:

```bash
rg "node\\.json" lp-core lp-base lp-cli lp-app lp-fw examples
rg "TODO|FIXME|XXX|dbg!|println!|eprintln!|unimplemented!|todo!|#\\[ignore\\]" lp-core lp-base lp-cli lp-app lp-fw examples
```

Use judgment on existing `println!` in tests or pre-existing TODOs; do not churn
unrelated old code. Remove only issues introduced by this plan unless a live
`node.json` reference must be updated for correctness.

Run formatting:

```bash
cargo +nightly fmt
```

Run tests:

```bash
cargo test -p lpc-source
cargo test -p lpfs
cargo test -p lpc-shared
cargo test -p lpc-engine
cargo test -p lpa-server
cargo test -p lp-cli
```

Run end-to-end perf-tool validation:

```bash
cargo run -p lp-cli -- profile examples/perf/fastmath --mode steady-render --max-cycles 80000000 --collect events --note m3-node-toml
```

The profile command pushes project files into the emulator-backed server, loads
the project, and drives frames. Treat this as the end-to-end proof that converted
examples load through the live runtime path.

Write `docs/roadmaps/2026-05-01-runtime-core/m3-legacy-source-migration/summary.md`
with exactly these sections:

```markdown
### What was built

- <one-line concrete change>

### Decisions for future reference

#### <short decision title>

- **Decision:** <one line>
- **Why:** <one or two lines>
- **Rejected alternatives:** <X because Y; Z because W>
- **Revisit when:** <condition, if any>
```

Record 0-5 useful decisions. Include decisions future readers might relitigate,
such as:

- `node.toml` wholesale switch with no long-term JSON compatibility loader.
- source-owned generic loading traits instead of `lpc-source -> lpfs`.
- examples are part of the migration and validated through `lp-cli profile`.

## Validate

Run from the repository root:

```bash
cargo +nightly fmt
cargo test -p lpc-source
cargo test -p lpfs
cargo test -p lpc-shared
cargo test -p lpc-engine
cargo test -p lpa-server
cargo test -p lp-cli
cargo run -p lp-cli -- profile examples/perf/fastmath --mode steady-render --max-cycles 80000000 --collect events --note m3-node-toml
```
