# Phase 5: Update Roadmap Docs, Cleanup, and Validation

Tag: `sub-agent: supervised`
Parallel: `-`

## Scope of phase

Clean up the completed reorganization, update roadmap documentation, and run the
final validation set for this milestone.

In scope:

- Update `docs/roadmaps/2026-05-01-runtime-core/m1-update-in-place.md` so it
  reflects the implemented update-in-place topology.
- Update `docs/roadmaps/2026-05-01-runtime-core/notes.md` if it still implies
  `lpl-*` crates remain part of the desired direction.
- Search the final diff for stale hook/install/lpl references.
- Remove stray temporary compatibility code, debug prints, commented-out code,
  and obsolete TODOs introduced by this plan.
- Run formatting and validation.
- Create `summary.md` in the plan directory.

Out of scope:

- Do not add new runtime features.
- Do not design the final `Engine` API.
- Do not reintroduce `lpl-*` compatibility crates.
- Do not weaken tests.

## Code Organization Reminders

- Keep documentation short and factual.
- Keep related module exports grouped together.
- Tests remain at the bottom of Rust source files.
- Do not leave temporary code in the final diff.

## Sub-agent Reminders

- Do not commit. The plan commits at the end as a single unit.
- Do not expand scope. Stay strictly within this phase.
- Do not suppress warnings or `#[allow(...)]` problems away; fix them.
- Do not disable, skip, or weaken existing tests to make the build pass.
- If validation fails with a non-obvious runtime bug, stop and report.
- Report back: files changed, validation run, result, and any deviations.

## Implementation Details

Read shared context first:

- `docs/roadmaps/2026-05-01-runtime-core/m1-update-in-place/00-notes.md`
- `docs/roadmaps/2026-05-01-runtime-core/m1-update-in-place/00-design.md`

Roadmap docs should communicate:

- `lpl-model` and `lpl-runtime` were removed.
- Legacy authored configs/source types now live under `lpc-source::legacy`.
- Legacy state/protocol/message types now live under `lpc-wire::legacy`.
- Concrete legacy runtime/node implementations now live under `lpc-engine`.
- `LegacyProjectRuntime` is preserved as the old runtime name.
- Hook registration and `lpl_runtime::install()` were removed.
- The next milestone can focus on the runtime owner/value-resolution contract.

Search for stale references:

```bash
rg "lpl_model|lpl-model|lpl_runtime|lpl-runtime|LegacyProjectHooks|set_project_hooks|with_hooks|project hooks not installed|project_hooks::install" .
rg "TODO|todo!|unimplemented!|dbg!|println!" .
```

Use judgment with existing TODOs: remove or rewrite only those introduced by
this plan or made stale by this plan. Do not churn unrelated historical TODOs.

Create:

`docs/roadmaps/2026-05-01-runtime-core/m1-update-in-place/summary.md`

Use this exact shape:

```text
### What was built

- <one-line bullet per concrete change>

### Decisions for future reference

#### <short title>

- **Decision:** <one line>
- **Why:** <one or two lines>
- **Rejected alternatives:** <X because Y; Z because W>
- **Revisit when:** <condition, omit if permanent>
```

Capture only real decisions. Likely decisions:

- remove `lpl-*` crates now rather than keeping shims;
- place legacy configs/source in `lpc-source` and state/protocol in `lpc-wire`;
- remove hook registration and make `LegacyProjectRuntime` direct again.

## Validate

Run from workspace root:

```bash
cargo +nightly fmt
cargo test -p lpc-engine
cargo check -p lpa-server
cargo test -p lpa-server --no-run
cargo check -p lpa-client
cargo check -p lp-cli
cargo test -p fw-tests --test scene_render_emu --test profile_alloc_emu
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
```

If the phase touches shader pipeline crates or firmware-visible runtime paths,
the ESP32 check is required by workspace rules. Fix all warnings and formatting
issues. If validation fails with a hard/non-obvious bug, stop and report.
