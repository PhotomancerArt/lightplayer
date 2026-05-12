# Phase 4: Cleanup, Validation, and Summary

## Scope of Phase

Review the completed M3.1 changes, run focused validation, and write the final
roadmap summary.

In scope:

- Check the diff for scope creep, temporary code, debug prints, disabled tests,
  and warning suppressions.
- Run final validation for the changed crates and narrow engine projection tests.
- Write `summary.md` for the roadmap plan directory.
- Add the M3.2 heavy snapshot handoff inventory to the summary.

Out of scope:

- New feature work.
- Retrying major design changes from earlier phases.
- New runtime product/buffer storage.
- Commits; the main agent handles commit decisions after review.

## Code Organization Reminders

- Keep cleanup edits minimal and directly tied to warnings/tests.
- Prefer fixing warnings over suppressing them.
- Place helper functions at the bottom of files if a small helper is required.
- Keep summary notes terse and grep-friendly.

## Sub-agent Reminders

- Do not commit. The plan commits at the end as a single unit.
- Do not expand scope. Stay strictly within "Scope of phase".
- Do not suppress warnings or `#[allow(...)]` problems away; fix them.
- Do not disable, skip, or weaken existing tests to make the build pass.
- If validation fails with a non-trivial bug, stop and report rather than
  debugging deeply.
- Report back: what changed, what was validated, and any deviations.

## Implementation Details

Plan directory:

- `docs/roadmaps/2026-05-01-runtime-core/m3.1-state-wire-projection/`

Changed code is expected primarily in:

- `lp-core/lpc-view/src/project/project_view.rs`
- `lp-core/lpc-view/tests/client_view.rs`
- `lp-core/lpc-wire/src/legacy/project/api.rs`
- `lp-core/lpc-wire/src/state/macros.rs`

Cleanup checks:

- Search the diff for:
  - `TODO`;
  - `todo!`;
  - `unimplemented!`;
  - `dbg!`;
  - `println!`;
  - `#[ignore]`;
  - new `#[allow(...)]`;
  - commented-out code.
- Existing TODOs outside touched lines are not automatically in scope, but do not
  add new ones unless the plan explicitly needs a future-work marker.

Write `summary.md` with:

```markdown
### What was built

- ...

### Decisions for future reference

#### SyncProjection names the client sync boundary

- **Decision:** ...
- **Why:** ...
- **Rejected alternatives:** ...

#### Heavy byte fields remain compatibility snapshots for M4

- **Decision:** ...
- **Why:** ...
- **Rejected alternatives:** ...
- **Revisit when:** M3.2 defines store-backed buffer/product identity.
```

The summary must include the M3.2 handoff inventory:

- texture bytes plus width/height/format;
- fixture lamp colors and mapping cells;
- output channel bytes;
- frame/version metadata;
- snapshot/reference/diff semantics to decide in M3.2.

## Validate

Run:

```bash
cargo test -p lpc-view
cargo test -p lpc-wire
cargo test -p lpc-engine --test partial_state_updates --test scene_update
```
