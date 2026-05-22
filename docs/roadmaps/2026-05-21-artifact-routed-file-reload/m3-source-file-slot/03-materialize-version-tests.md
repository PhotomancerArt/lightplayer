# Phase 03 — Materialize + version tests

**Dispatch:** [sub-agent: yes, model: composer-2.5-fast, parallel: -]

## Scope of phase

Read transient UTF-8 text and compute effective revision for M4 file-bump scenarios.

**In scope:**

- `MaterializedSource { version, text, diagnostic_name }`
- `SourceDiagnosticCtx { containing_file, slot_path }`
- `MaterializeError` (`Unsupported`, `Utf8`, `MissingInlineBody`, `Artifact`)
- `materialize_source(store, fs, ref, slot, ctx)`
- Effective version: `max(slot.revision(), artifact.revision())` for file;
  `slot.revision()` for inline
- Diagnostic names: authored path (file); `{containing_file}:source.{ext}` (inline)
- `Url` ref → `MaterializeError::Unsupported`
- Tests: file read, inline read, fs modify bump without slot edit

**Out of scope:** ChangeSet / AssetView (M5), engine integration (M6).

## Implementation Details

### File mode

1. `store.read_bytes(artifact_id, fs)`
2. UTF-8 decode
3. `version = slot.revision().max(store.revision(artifact_id))`
4. `diagnostic_name = authored_path`

### Inline mode

1. Text from `slot.inline_value()` at materialize time
2. `version = slot.revision()`
3. `diagnostic_name` from `SourceDiagnosticCtx`

### Gate test (M4 preview)

File content changes via `apply_fs_changes` → materialize version increases while
authored slot revision unchanged.

## Validate

```bash
cargo test -p lpc-node-registry materialize
cargo test -p lpc-node-registry
```
