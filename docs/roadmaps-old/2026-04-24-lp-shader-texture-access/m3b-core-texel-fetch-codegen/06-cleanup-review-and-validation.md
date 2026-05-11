# Phase 6: Cleanup, Review, And Validation

## Scope of phase

Clean up the M3b implementation, run final validation, and write the plan
summary.

In scope:

- Remove stray temporary code, debug prints, stale placeholder references, and
  commented-out code.
- Ensure diagnostics and tests no longer reference the valid-`texelFetch` M3b
  placeholder.
- Run formatting and focused validation.
- Add `summary.md` for this roadmap plan directory.

Out of scope:

- Do not add new feature scope.
- Do not broaden backend coverage beyond the agreed M3b targets unless needed
  to validate existing changes.
- Do not commit.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Sub-agent Reminders

- Do not commit. The plan commits at the end as a single unit.
- Do not expand scope. Stay strictly within "Scope of phase".
- Do not suppress warnings or `#[allow(...)]` problems away. Fix them.
- Do not disable, skip, or weaken existing tests to make the build pass.
- If something blocks completion, stop and report rather than improvising.
- Report back: what changed, what was validated, and any deviations.

## Implementation Details

Read:

- `docs/roadmaps/2026-04-24-lp-shader-texture-access/m3b-core-texel-fetch-codegen/00-notes.md`
- `docs/roadmaps/2026-04-24-lp-shader-texture-access/m3b-core-texel-fetch-codegen/00-design.md`
- The git diff for files changed by phases 1-5

Cleanup checks:

- Search the diff for:
  - `TODO`
  - `dbg!`
  - `println!`
  - `eprintln!`
  - `unimplemented!`
  - `todo!`
  - the placeholder text `data path is implemented in M3b`
  - new `#[allow(...)]`
  - `#[ignore]`
- Remove temporary or stale items. If an existing unrelated TODO appears in the
  diff context, do not churn it.
- Make sure no valid `texelFetch` test still expects the placeholder diagnostic.
- Keep unsupported diagnostic tests for missing spec, nonzero LOD, dynamic LOD,
  and unsupported operands.

Run formatting:

```bash
cargo +nightly fmt
```

Run focused validation:

```bash
cargo test -p lpir compiler_config
cargo test -p lps-frontend sampler2d_metadata_tests
cargo test -p lps-filetests --test filetests -- textures
cargo check -p lps-frontend -p lp-shader -p lps-filetests
```

If one command is too broad or the test harness uses a different texture filter,
use the closest focused command and report the exact command.

Write
`docs/roadmaps/2026-04-24-lp-shader-texture-access/m3b-core-texel-fetch-codegen/summary.md`
with:

```markdown
### What was built

- <one-line concrete change>

### Decisions for future reference

#### Safe texelFetch bounds by default

- **Decision:** Generate clamp-to-edge bounds guards for `texelFetch` by default, with an explicit unchecked compiler option.
- **Why:** Default behavior must avoid arbitrary shared-memory reads; unchecked mode exists for performance measurement.
- **Rejected alternatives:** Always unchecked (unsafe); runtime trap (not supported by current LPIR/runtime surface).
- **Revisit when:** A richer runtime validation/trap mechanism exists or measured clamp cost requires a different policy.
```

Add 0-4 additional decisions only if they capture real forks in the road not
already obvious from `00-design.md`.

## Validate

Run from workspace root:

```bash
cargo +nightly fmt
cargo test -p lpir compiler_config
cargo test -p lps-frontend sampler2d_metadata_tests
cargo test -p lps-filetests --test filetests -- textures
cargo check -p lps-frontend -p lp-shader -p lps-filetests
```

