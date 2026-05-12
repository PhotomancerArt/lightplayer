# Phase 4: Cleanup And Validation

## Scope Of Phase

In scope:

- Remove remaining stale imports, dead modules, and legacy-only tests.
- Audit for legacy project sync/detail vocabulary leaks.
- Record canonical rebuild follow-ups for M3/M4/M5.
- Run final focused validation.

Out of scope:

- Implementing canonical project sync.
- Rebuilding project view or debug UI.
- Runtime slot roots.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- Keep only TODOs that point to a specific future milestone.
- Tests belong at the bottom of files.
- Avoid commented-out experiments; delete them or record context in plan notes.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Audit commands:

```bash
rg -n "LegacyProjectResponse|LegacySerializableProjectResponse|LegacyNodeDetail|LegacySerializableNodeDetail|LegacyNodeState|LegacyWireNodeSpecifier|legacy_detail|detail_projection|compatibility_projection" lp-core lp-app lp-cli
rg -n "LegacyCompatBytes|legacy compat|legacy GetChanges|node detail" lp-core lp-app lp-cli
```

Expected results:

- No active code depends on legacy project response/detail/state types.
- Any remaining `legacy` hits are either unrelated legacy reference modules,
  old archived docs, or explicit TODOs with milestone references.
- Resource sync docs no longer describe legacy project response as the owner of
  resource summaries/payloads.
- Deleted tests are noted in `00-notes.md` or a phase summary so M3-M6 can
  restore coverage intentionally.

Final notes to write:

- Create `summary.md` in this plan directory with:
  - deleted legacy surfaces,
  - retained/renamed surfaces,
  - disabled entry points,
  - tests removed,
  - validation commands run,
  - M3/M4/M5 follow-up inventory.

## Validate

Run:

```bash
cargo fmt
cargo test -p lpc-wire
cargo check -p lpc-wire --features schema-gen
cargo check -p lpc-engine
cargo check -p lpc-view
cargo test -p lpc-view
cargo check -p lpa-client
cargo check -p lpa-server
cargo check -p lp-cli
git diff --check
```

Do not run `cargo test --workspace` or `cargo build --workspace`.

