# Phase 4: Cleanup And Validation

## Scope Of Phase

Polish the debug UI pass and validate it.

In scope:
- Remove stale helper names and temporary comments.
- Make labels consistent with current domain vocabulary: node, slot, value, product, resource, shape.
- Ensure UI still works when project sync has not arrived yet.
- Keep layout usable at normal laptop window sizes.

Out of scope:
- Final design polish.
- New protocol features.

## Code Organization Reminders

- Keep concept-per-file organization if files were split.
- Tests stay at the bottom of files.
- No broad unrelated refactors.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Expected cleanup:
- Review all changed debug UI files for obvious duplication.
- Confirm product/resource skeletons use clear copy and disabled buttons where detail is not implemented.
- Confirm missing-state roots are treated as normal.
- Confirm current recursive slot debug remains available through the inspector.

## Validate

```bash
cargo fmt --check
cargo test -p lp-cli --no-run
cargo test -p lpc-view
```

Manual validation:

```bash
RUST_BACKTRACE=1 just demo
```

Expected manual result:
- Main area shows node cards.
- Right panel shows nodes/resources/shapes.
- Clicking nodes/resources/shapes updates the lower debug detail.
- No repeated shader recompilation when project/config is unchanged.

