# Phase 8: Cleanup, Validation, And Summary

## Scope of phase

Final review and cleanup for the project artifact initial-load plan.

In scope:

- Remove temporary code, stale TODOs introduced by this plan, debug prints, and stale comments.
- Ensure rustdocs for the key concepts match the final implementation.
- Run focused validation and the best feasible broader host checks.
- Write `summary.md` in this plan directory.
- Record future work discoveries in `future.md` if needed.

Out of scope:

- Do not start new architecture work.
- Do not implement future namespace/wire data model items.
- Do not archive the plan directory unless the user explicitly asks at this point; this repo may prefer keeping active plan docs visible until commit/review.

## Code Organization Reminders

- Follow the repo rule: top to bottom is most important to least important, with tests at the bottom of each Rust file.
- Prefer one concept per file and keep related functionality grouped together.
- Keep helper functions below the public/primary API they support.
- Any temporary code must have a searchable TODO comment and should be removed by the cleanup phase.
- Preserve no_std compatibility in `lpc-model`, `lpc-source`, `lpc-engine`, and shader/runtime paths. Do not add std gates to compile/execute paths.

## Codex / Worker Reminders

- Do not commit. The plan commits at the end as a single unit unless the user explicitly says otherwise.
- Do not expand scope. Stay strictly within this phase.
- Do not suppress warnings or add `#[allow(...)]` to make the build pass. Fix the issue.
- Do not disable, skip, or weaken existing tests.
- If blocked by ambiguity or an unexpected design issue, stop and report back rather than improvising.
- Report back with: what changed, what was validated, and any deviations from this phase.

## Implementation Details

Search for stale language and temporary code:

```bash
rg -n "TODO|debug|println!|dbg!|project.json|SrcNodeConfig|SrcArtifactSpec|NodeSpec|TextureConfig|ShaderConfig|OutputConfig|FixtureConfig|legacy_src_dirs" \
  lp-core/lpc-model lp-core/lpc-source lp-core/lpc-engine lp-core/lpc-view lp-core/lpc-wire examples docs/plans/2026-05-05-project-artifact-initial-load
```

Some hits may be historical docs or intentionally retained legacy modules. Review them; do not mechanically delete.

`summary.md` should include:

```markdown
# Summary

## What was built

- ...

## Decisions for future reference

#### Relative NodeLoc syntax

- **Decision:** ...
- **Why:** ...
- **Rejected alternatives:** ...
```

Capture 2-5 decisions max. Good candidates: artifact-rooted project load, relative-only dot `NodeLoc`, `Def` suffix, no artifact merge in this plan, keeping compatibility wire temporarily.

## Validate

Run the strongest feasible validation set. At minimum:

```bash
cargo test -p lpc-model
cargo test -p lpc-source
cargo test -p lpc-engine --test scene_render --test partial_state_updates --test get_changes_resource_projection
```

If the change touched server/CLI callers, also run:

```bash
cargo test -p lpa-server --no-run
cargo test -p lp-cli --no-run
```

If shader pipeline behavior changed, consider the AGENTS validation commands appropriate to the touched surface. Do not run `cargo test --workspace`.
