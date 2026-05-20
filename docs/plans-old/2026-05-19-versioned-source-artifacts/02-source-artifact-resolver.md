# Phase 2: Source Artifact Resolver

- **parallel:** -
- **sub-agent:** supervised

## Scope Of Phase

Introduce the runtime source identity, version, and lazy materialization layer
that shader and compute nodes will use later.

In scope:

- Add resolved shader source identity types.
- Add opaque source version/update types.
- Add a source resolver service that can check unchanged versus changed.
- Register file-backed and inline shader source identities.
- Read source text lazily only when materialization is needed.
- Add unit tests for version comparison and lazy-read behavior.

Out of scope:

- Replacing shader node constructors.
- Project loader one-file support.
- Filesystem watcher integration.
- General binary/image materialization.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- Keep related functionality grouped together.
- Put helpers lower in the file when that improves readability.
- Mark any temporary code with a clear `TODO`.
- Keep tests at the bottom of Rust files.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

### Add source identity/version types

Suggested files:

- `lp-core/lpc-engine/src/artifact/artifact_source.rs`
- `lp-core/lpc-engine/src/artifact/source_resolver.rs`

Suggested core types:

```rust
pub enum ShaderLanguage {
    Glsl,
}

pub struct SourceVersion {
    artifact: ArtifactId,
    content_revision: Revision,
    abi_revision: Revision,
}

pub enum SourceUpdate<T> {
    Unchanged(SourceVersion),
    Changed(VersionedSource<T>),
}

pub struct VersionedSource<T> {
    pub version: SourceVersion,
    pub value: T,
}

pub struct ShaderSourceText {
    pub language: ShaderLanguage,
    pub text: String,
}
```

Keep `SourceVersion` opaque from node code. Provide equality/comparison and
small accessors only where needed for diagnostics.

### Extend artifact locations

File:

- `lp-core/lpc-engine/src/artifact/artifact_location.rs`

Add enough identity to distinguish:

- referenced files,
- inline node definitions,
- inline shader source inside an owning node artifact.

Prefer stable authored coordinates over process-local node IDs for inline
locations. For example:

```rust
InlineShaderSource {
    owner: LpPathBuf,
    node_path: String,
    field: String,
}
```

If the owning artifact may later be `lib:`, use a resolved owner location rather
than assuming `LpPathBuf` is always a filesystem path.

### Evolve artifact store carefully

File:

- `lp-core/lpc-engine/src/artifact/artifact_store.rs`

The current store is NodeDef-payload-specific. Avoid a risky rewrite if a
sibling source table is cleaner.

Acceptable implementation paths:

- Add a separate `SourceArtifactStore`/`SourceResolver` that uses
  `ArtifactLocation` and `ArtifactId` but does not store `NodeDef` payloads.
- Or split `ArtifactStore` into identity/revision bookkeeping plus typed
  payload caches if the change remains contained.

Required behavior:

- Acquiring the same resolved location returns the same artifact/source identity.
- The resolver can report the current version without reading source bytes.
- Materializing changed file-backed source reads UTF-8 bytes on demand.
- Materializing inline source returns the stored inline text.
- Delete/missing source should produce a useful error and should not panic.

### Read service

The project loader currently receives an `ArtifactReadRoot` by reference and
does not preserve it on `Engine`. To support lazy reads after load, add a narrow
source read service owned by or reachable from the engine.

Possible approaches:

- Store an `Rc<dyn ArtifactReadRoot>`-like service if object safety and
  lifetime constraints work in `no_std + alloc`.
- Store a project source root wrapper in the engine/project runtime.
- For the first implementation, keep a source resolver populated with inline
  text and resolved path identities, and give it a read service during engine
  construction/loading.

Do not make shader nodes own the filesystem root.

### Tests

Add tests for:

- path source resolves relative to owning artifact location,
- inline GLSL source gets a stable source identity,
- unchanged source check does not call `read_file`,
- changed source materialization reads text once,
- source version changes when content revision changes,
- invalid UTF-8 reports a source materialization error.

## Validate

Run:

```bash
cargo test -p lpc-engine artifact --lib
cargo test -p lpc-engine source --lib
cargo check -p lpc-engine
cargo test -p lpc-model
```

If exact test filters do not match new module names, run the nearest targeted
`cargo test -p lpc-engine <module-or-test-name> --lib` commands plus:

```bash
cargo check -p lpc-engine
```
