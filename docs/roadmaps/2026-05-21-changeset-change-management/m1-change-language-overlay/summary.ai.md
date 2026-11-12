# M1 Summary — Change Language + Overlay Lifecycle

## Status

Implemented on branch `codex/incremental-artifact-reload`.

## Delivered

- `src/change/` — `ChangeSet`, `ArtifactChange`, `ArtifactOp`, `ArtifactTarget`, `ChangeError`
- `ChangeOverlay` on `NodeDefRegistry` with apply/discard API
- M1 op apply: `SetBytes`, `Delete`; slot ops return `UnsupportedOp`
- Tests: D1, D3 + serde round-trip + path/implicit-create coverage

## Validation

```bash
cargo test -p lpc-node-registry
cargo test -p lpc-node-registry --test fs_change_semantics
cargo test -p lpc-node-registry --test overlay_lifecycle
cargo clippy -p lpc-node-registry --all-targets --no-deps -- -D warnings
```

## Next

M2 effective projection — wire `NodeDefView` and artifact reads through overlay.
