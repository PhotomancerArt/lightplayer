# Phase 4: Syntax Differences And Cleanup

## Scope Of Phase

In scope:

- remove obsolete serde-probe helpers from `NodeDef`
- update comments/docs that still say definition loading is serde-owned
- record any authored TOML syntax differences
- run focused validation for M3

Out of scope:

- removing serde derives from all model types
- deleting serde dependencies
- fixing unrelated tests outside the definition-loading path

## Code Organization Reminders

- Keep `future.md` for real follow-up ideas.
- Keep `summary.md` concise and concrete.
- Do not leave commented-out serde experiments.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Search for stale M3-local serde usage:

```bash
rg -n "NodeDefKindProbe|DeserializeOwned|toml::from_str|toml::to_string|from_toml_str" \
  lp-core/lpc-model/src/nodes \
  lp-core/lpc-engine/src/engine/project_loader.rs \
  lp-core/lpc-shared/src/project/builder.rs
```

Expected final state:

- no serde `NodeDefKindProbe`
- no serde variant parse helper in `NodeDef`
- project loader reads definitions through slot registry
- project builder writes slotted node payloads through slot registry
- any remaining `toml::from_str` in tests is either parsing to `toml::Value` or
  explicitly outside this M3 path

Write `summary.md` with:

- what changed
- syntax compatibility notes
- remaining serde usage intentionally left for M4

## Validate

```bash
cargo test -p lpc-model node_def
cargo test -p lpc-model slot_codec
cargo test -p lpc-engine project_loader
cargo test -p lpc-engine project_read
cargo test -p lpc-shared project::builder
git diff --check
```
