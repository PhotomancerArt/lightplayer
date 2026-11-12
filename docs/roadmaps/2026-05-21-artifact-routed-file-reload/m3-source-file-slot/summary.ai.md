# M3 Summary — SourceFileSlot + SourceFileRef

## What was built

- **`SourceFileSlot`** (`lpc-model`): authored file-or-inline UTF-8 source with custom
  codec (`$path`, shorthand string, extension-key inline table).
- **`SourceFileRef`** + **`resolve_source_file`**: handle-only resolved refs; file mode
  acquires artifacts in M1 `ArtifactStore`.
- **`MaterializedSource`** + **`materialize_source`**: transient UTF-8 read via
  `read_bytes`; effective version `max(slot, artifact)` for files, slot-only for inline.
- **`SourceDiagnosticCtx`**: stable diagnostic labels for compile errors.
- Tests in `lpc-model` (codec) and `lpc-node-registry` (resolve, materialize, file bump).

Production `ShaderSource` / `lpc-engine` paths unchanged (M6 cutover).

## Decisions for future reference

#### Explicit resolve vs parse-time acquire

- **Decision:** Driver calls `resolve_source_file` after parse; refs hold handles only.
- **Why:** Separates authored shape from artifact lifecycle; same pattern as M2 registry.
- **Revisit when:** M6 engine wires resolve into node load.

#### Effective version = max(slot, artifact) for files

- **Decision:** File edits bump artifact revision even when node TOML unchanged.
- **Why:** M4 fs-change scenario depends on version change without def diff.
- **Revisit when:** M5 AssetView may add a third revision source.

#### URL stub unsupported in M3

- **Decision:** `SourceFileRef::Url` exists; materialize returns `Unsupported`.
- **Why:** Reserve enum shape without committing fetch/cache semantics.

Plan: `docs/roadmaps/2026-05-21-artifact-routed-file-reload/m3-source-file-slot/`
