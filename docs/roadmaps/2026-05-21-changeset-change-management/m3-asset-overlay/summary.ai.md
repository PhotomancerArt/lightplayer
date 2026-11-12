# M3 Summary — File Ops + Asset Reads

## Status

Implemented (`81ff051b`) on branch `codex/incremental-artifact-reload`.

Plan folder backfilled post-hoc; execution skipped numbered phase files.

## Delivered

- `SetBytes` / `Delete` apply on overlay (via `change/apply.rs` + registry routing)
- `read_effective_bytes` — overlay before committed store/fs
- `materialize_source` — checks `ChangeOverlay` before store read
- `NodeDefRegistry::materialize_source` wrapper
- `source_bridge` passes overlay into materialize path

## Tests

`lp-core/lpc-node-registry/tests/asset_overlay.rs` — C4a–d:

- C4a — add asset via overlay implicit create
- C4b — replace asset bytes
- C4c — replace GLSL; def TOML unchanged in committed cache
- C4d — delete asset via overlay

Also covered indirectly: `overlay_lifecycle.rs` (SetBytes/Delete lifecycle).

## Validation

```bash
cargo test -p lpc-node-registry --test asset_overlay
cargo test -p lpc-node-registry --test overlay_lifecycle
cargo test -p lpc-node-registry
```

## Deferred to M5+

- Source revision bump after commit (C4c post-commit expectation)
- Commit promotion of overlay assets to store/fs

## Next

M4 slot ops + TOML serialize; M5 commit promotion.
