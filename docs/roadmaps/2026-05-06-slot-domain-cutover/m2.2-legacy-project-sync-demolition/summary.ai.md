# M2.2 Summary

## Result

M2.2 removed the active legacy project sync/detail path and left the repo in a
cleaner in-between state for M3 canonical sync.

Completed:

- Deleted `lpc-wire` legacy project response, node detail, node state, and
  compatibility byte-field modules.
- Replaced `WireProjectRequest::GetChanges` with an explicit disabled project
  sync request until M3 rebuilds the canonical protocol.
- Removed engine `get_changes` / node-detail projection and renamed the
  retained authoring lookup to `SourceAuthoringIndex`.
- Gutted `ProjectView` into a minimal shell that tracks frame id, node entries,
  watched slot roots, and resource cache.
- Removed old node-specific debug UI panels and left the CLI debug UI as a
  small placeholder for the M5 generic slot/resource inspector.
- Removed legacy project-sync tests from core/app crates.
- Replaced firmware scene/profile sync tests with ignored placeholders so their
  test targets still exist while M3 rebuilds canonical firmware coverage.
- Kept generic wire envelopes, resource summary/payload types, source loading,
  engine runtime loading, and transport message plumbing.

## Follow-Up

- M3 should define and implement canonical project sync on top of slot roots,
  resource summaries, and explicit resource payload interest.
- M4 should rebuild `ProjectView` around `SlotMirrorView` and the canonical
  resource mirror.
- M5 should rebuild the debug UI as a generic slot/resource inspector instead of
  node-specific state panels.
- M6 should apply slot exposure to real source defs, runtime config/state, and
  output/resource roots.
- Firmware render/profile tests need to be restored on the canonical sync path;
  the placeholder files preserve the target names for now.

## Validation

- `cargo fmt`
- `git diff --check`
- `cargo check`
- `cargo test -p lpc-wire`
- `cargo check -p lpc-wire --features schema-gen`
- `cargo check -p lpc-engine`
- `cargo test -p lpc-engine --lib`
- `cargo check -p lpc-view`
- `cargo test -p lpc-view`
- `cargo check -p lpa-client`
- `cargo test -p lpa-client`
- `cargo check -p lpa-server`
- `cargo test -p lpa-server`
- `cargo check -p lp-cli`
- `cargo test -p lp-cli`
- `cargo test -p fw-tests --no-run`
- `cargo clippy -p lpc-wire -p lpc-engine -p lpc-view -p lpa-client -p lpa-server -p lp-cli -p fw-tests --all-targets -- -D warnings`
- Legacy-name audits over `lp-core`, `lp-app`, `lp-cli`, and `lp-fw`
