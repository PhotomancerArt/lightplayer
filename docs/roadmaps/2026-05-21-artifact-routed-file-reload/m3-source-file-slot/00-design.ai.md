# M3 Design — SourceFileSlot + SourceFileRef

## Scope

Add **`SourceFileSlot`** (`lpc-model`) and **`SourceFileRef` + materialize**
(`lpc-node-registry`) in the parallel stack. Production `ShaderSource` unchanged
until M6.

## Driver flow

```
TOML → SourceFileSlot
         ↓ resolve_source_file(store, containing_file, slot, frame)
       SourceFileRef (no text)
         ↓ materialize_source(store, fs, ref, slot, ctx)
       MaterializedSource { version, text, diagnostic_name }
```

Driver owns `ArtifactStore` + `LpFs`. Resolve acquires file artifacts; materialize
reads bytes transiently via `read_bytes`.

## File structure

```
lp-core/lpc-model/src/slots/
  source_file.rs           # SourceFileSlot, backing, FieldSlot, codec hooks

lp-core/lpc-node-registry/src/source/
  mod.rs
  source_file_ref.rs
  materialized_source.rs
  resolve.rs
  materialize.rs
```

## Types

### SourceFileSlot (authored)

```rust
pub enum SourceFileBacking {
    Path(SourcePath),
    Inline { extension: String, text: String },
}

pub struct SourceFileSlot { backing, revision }
```

Custom codec id: `lp::slots::SourceFileCodec`.

TOML forms: `$path`, shorthand string, extension-key inline table.

### SourceFileRef (resolved)

```rust
pub enum SourceFileRef {
    File { artifact_id, authored_path, resolved_path, extension },
    Inline { extension, slot_revision },
    Url { .. },  // stub → Unsupported in M3
}
```

### MaterializedSource

```rust
pub struct MaterializedSource {
    pub version: Revision,
    pub text: String,
    pub diagnostic_name: String,
}
```

**Effective version:** `max(slot.revision(), artifact.revision())` for file mode;
slot revision only for inline.

## Validation

```bash
cargo +nightly fmt --all
cargo test -p lpc-model source_file
cargo test -p lpc-node-registry
cargo clippy -p lpc-node-registry --all-targets --no-deps -- -D warnings
```

## Plan phases

| # | Phase | Dispatch |
|---|-------|----------|
| 01 | SourceFileSlot type + codec | composer-2.5-fast |
| 02 | SourceFileRef + resolve | composer-2.5-fast |
| 03 | Materialize + version tests | composer-2.5-fast |
| 04 | Cleanup + summary | supervised |
