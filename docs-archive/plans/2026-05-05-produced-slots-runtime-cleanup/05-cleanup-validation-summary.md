# Cleanup, Validation, And Summary

## Scope of phase

Finish the implementation by cleaning up temporary names, running final
validation, writing the summary, and preparing the commit.

In scope:

- Remove temporary aliases, stale TODOs, commented-out experiments, and debug
  prints.
- Search for stale terms such as `PropPath`, `PropNamespace`, `NodeRuntime`,
  `RuntimePropAccess`, `RuntimeOutputAccess`, `NodeInput`, and `NodeOutput`.
- Keep any remaining historical mentions only when clearly marked as history.
- Run final validation.
- Write `summary.md`.
- Move the standalone plan to `docs/plans-old/` after completion.
- Create a single conventional commit if validation passes.

Out of scope:

- New architecture changes not required to finish prior phases.
- Push/PR/CI watch unless requested separately.

## Code organization reminders

- Tests stay at the bottom of Rust files.
- Rustdocs should describe semantic meaning, not this plan.
- Do not stage unrelated user changes.

## Sub-agent reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation details

Relevant searches:

```bash
rg "PropPath|PropNamespace|NodeRuntime|RuntimePropAccess|RuntimeOutputAccess|NodeInput|NodeOutput|NodeLoc" lp-core lp-cli docs/plans/2026-05-05-produced-slots-runtime-cleanup
rg "TODO|dbg!|println!|eprintln!" lp-core/lpc-model lp-core/lpc-engine lp-core/lpc-source lp-core/lpc-view lp-core/lpc-wire
```

Expected changes:

- `summary.md` should include what was built and key decisions for future
  reference.
- Since this is a standalone plan, archive it under `docs/plans-old/` when the
  implementation is complete.
- Commit body should include `Plan: docs/plans-old/2026-05-05-produced-slots-runtime-cleanup`.

## Validate

```bash
cargo test -p lpc-model
cargo test -p lpc-source
cargo test -p lpc-engine
cargo test -p lpc-view
cargo test -p lpc-wire
cargo check -p lpa-server
cargo test -p lpa-server --no-run
```
