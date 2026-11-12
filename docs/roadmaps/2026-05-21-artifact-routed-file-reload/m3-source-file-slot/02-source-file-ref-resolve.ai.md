# Phase 02 — SourceFileRef + resolve

**Dispatch:** [sub-agent: yes, model: composer-2.5-fast, parallel: -]

## Scope of phase

Add resolved handle type and explicit resolve step in `lpc-node-registry`.

**In scope:**

- `SourceFileRef` enum (`File` / `Inline` / `Url` stub)
- `ResolveError`
- `resolve_source_file(store, containing_file, slot, frame)`
- Path resolve via `resolve_node_specifier` (reuse registry helper)
- File mode acquires `ArtifactLocation::file(resolved_path)` in store
- `pub(crate) use def_walker::resolve_node_specifier` from registry
- Unit tests: path acquire + inline revision

**Out of scope:** Reading bytes, version combine, `ShaderDef` cutover.

## Implementation Details

### `source/source_file_ref.rs`

```rust
pub enum SourceFileRef {
    File { artifact_id, authored_path, resolved_path, extension },
    Inline { extension, slot_revision },
    Url { url },
}
```

### `source/resolve.rs`

- `SourceFileBacking::Path` → resolve relative to `containing_file`, acquire artifact
- `SourceFileBacking::Inline` → no store acquire; carry slot revision + extension
- Extension hint from file path suffix for `File` variant

## Validate

```bash
cargo test -p lpc-node-registry resolve
cargo check -p lpc-node-registry
```
