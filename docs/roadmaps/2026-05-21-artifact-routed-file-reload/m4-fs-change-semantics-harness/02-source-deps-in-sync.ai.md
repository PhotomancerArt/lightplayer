# Phase 02 — Source deps + revisions inside sync

**Dispatch:** [sub-agent: yes, model: composer-2.5-fast, parallel: -]

## Scope of phase

GLSL/SVG file edits produce **`source_revisions`** from **`sync` itself** — no
harness index.

**In scope:**

- `registry/source_deps.rs` — record resolved source paths per def on load/re-derive
- `registry/source_bridge.rs` — internal; Shader/SvgPath → M3 materialize version
- During `sync`: when a changed path is a **source file** (not def TOML), find
  dependent defs, re-materialize, append `SourceRevisionBump` to result
- Test: S2 shape — glsl change only → empty `def_updates`, non-empty `source_revisions`

**Out of scope:** DefChangeDetail, integration scenarios, ChangeSet variant.

## Design

On each loaded/re-derived def entry, store:

```rust
struct SourceDep {
    resolved_path: LpPathBuf,  // or ArtifactId after acquire
    last_version: Revision,
}
// Vec<SourceDep> or small inline vec on entry — file-backed sources only
```

When `sync` applies `FsChange` to path P:

1. Re-derive defs if P is a def artifact (existing logic)
2. Else if P matches any entry's `SourceDep.resolved_path`, re-materialize and
   compare version → maybe push bump

## Memory

- Small **`Vec`** of deps per def (typically 0–1 for M4 fixtures)
- `source_revisions` in result: sparse `Vec`

## Validate

```bash
cargo test -p lpc-node-registry source
cargo test -p lpc-node-registry
```
