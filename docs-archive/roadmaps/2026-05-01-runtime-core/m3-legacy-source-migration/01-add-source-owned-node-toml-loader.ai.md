# Phase 1: Add Source-Owned Node TOML Loader

## Metadata

- **sub-agent:** yes
- **model:** composer-2
- **parallel:** -

## Scope of Phase

Add the reusable legacy `node.toml` loading foundation in `lpc-source`, plus the
`lpfs` trait implementations needed to use it with project filesystems.

In scope:

- Add `lpc-source::legacy` helpers for the `node.toml` sentinel filename.
- Move/copy legacy node kind-from-path and node-directory detection policy into
  `lpc-source`.
- Add source-owned read/discovery traits for loading legacy node config TOML.
- Add a typed loader that returns `(LpPathBuf, Box<dyn NodeConfig>)`.
- Add `lpfs` implementations for the new source-owned traits, matching the
  existing `ArtifactReadRoot` pattern.
- Add focused unit tests in `lpc-source` and/or `lpfs`.

Out of scope:

- Do not update `lpc-engine` to use the new loader yet.
- Do not convert builders, templates, tests, or examples.
- Do not add `lpfs` as a dependency of `lpc-source`.
- Do not add a long-term `node.json` compatibility loader.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place public entry points and abstract traits first, support code below them,
  helpers at the bottom, and tests at the bottom of each file.
- Keep related functionality grouped together.
- Any temporary code must have a `TODO` comment with a clear follow-up.

## Sub-Agent Reminders

- Do not commit. The plan commits at the end as a single unit.
- Do not expand scope. Stay strictly within this phase.
- Do not suppress warnings or add `#[allow(...)]` to hide problems.
- Do not disable, skip, or weaken existing tests.
- If blocked by an unexpected design issue, stop and report instead of
  improvising.
- Report back: files changed, validation run, validation result, and deviations.

## Implementation Details

Read the shared context first:

- `docs/roadmaps/2026-05-01-runtime-core/m3-legacy-source-migration/00-notes.md`
- `docs/roadmaps/2026-05-01-runtime-core/m3-legacy-source-migration/00-design.md`

Relevant existing files:

- `lp-core/lpc-source/src/legacy/mod.rs`
- `lp-core/lpc-source/src/legacy/nodes/mod.rs`
- `lp-core/lpc-source/src/legacy/nodes/kind.rs`
- `lp-core/lpc-source/src/legacy/nodes/shader/config.rs`
- `lp-core/lpc-source/src/legacy/nodes/texture/config.rs`
- `lp-core/lpc-source/src/legacy/nodes/fixture/config.rs`
- `lp-core/lpc-source/src/legacy/nodes/output/config.rs`
- `lp-base/lpfs/src/lpc_model_artifact.rs`
- `lp-base/lpfs/src/lib.rs`
- `lp-base/lpfs/src/lp_fs.rs`

Suggested module shape:

```text
lp-core/lpc-source/src/legacy/
├── mod.rs
├── node_config_file.rs
└── node_loader.rs

lp-base/lpfs/src/
├── lib.rs
└── lpc_source_legacy.rs
```

`node_config_file.rs` should own constants and path helpers. A reasonable first
shape:

```rust
pub const LEGACY_NODE_CONFIG_FILE: &str = "node.toml";

pub fn legacy_node_config_path(path: &LpPath) -> LpPathBuf {
    path.to_path_buf().join(LEGACY_NODE_CONFIG_FILE)
}

pub fn legacy_node_kind_from_path(path: &LpPathBuf) -> Result<NodeKind, LegacyNodePathError>;

pub fn legacy_is_node_directory(path: &LpPathBuf) -> bool;
```

The existing implementation to move/copy is in
`lp-core/lpc-engine/src/legacy_project/legacy_loader.rs`. Keep the behavior:
directory suffixes `.texture`, `.shader`, `.output`, `.fixture` map to
`NodeKind`.

Avoid depending on `lpc-engine::Error` from `lpc-source`. Define a small source
error type for path/kind and load errors. It can be minimal, but should preserve
enough context for `lpc-engine` to map errors later:

```rust
#[derive(Debug)]
pub enum LegacyNodeLoadError<E> {
    Io { path: LpPathBuf, error: E },
    InvalidPath { path: LpPathBuf, reason: &'static str },
    UnknownKind { path: LpPathBuf, suffix: alloc::string::String },
    Parse { path: LpPathBuf, error: toml::de::Error },
}
```

Use a source-owned trait instead of depending on `lpfs`:

```rust
pub trait LegacyNodeReadRoot {
    type Err;

    fn read_file(&self, path: &LpPath) -> Result<alloc::vec::Vec<u8>, Self::Err>;
    fn list_dir(&self, path: &LpPath, recursive: bool) -> Result<alloc::vec::Vec<LpPathBuf>, Self::Err>;
}
```

Add functions along these lines:

```rust
pub fn discover_legacy_node_dirs<R>(
    fs: &R,
    src_path: &LpPath,
) -> Result<Vec<LpPathBuf>, LegacyNodeLoadError<R::Err>>
where
    R: LegacyNodeReadRoot + ?Sized;

pub fn load_legacy_node_config<R>(
    fs: &R,
    path: &LpPath,
) -> Result<(LpPathBuf, Box<dyn NodeConfig>), LegacyNodeLoadError<R::Err>>
where
    R: LegacyNodeReadRoot + ?Sized;
```

The loader should:

1. Build `<node-dir>/node.toml`.
2. Read it through `LegacyNodeReadRoot`.
3. Determine kind from directory suffix.
4. Parse TOML with `toml::from_str`.
5. Return the typed config boxed as `Box<dyn NodeConfig>`.

`lpc-source` already uses `alloc` and `toml`, so keep this `no_std` +
`alloc` compatible. Do not introduce `std` APIs in default paths.

In `lpfs`, add a module that implements `LegacyNodeReadRoot` for the same types
as `ArtifactReadRoot`:

- `LpFsMemory`
- `LpFsView`
- `dyn LpFs`
- `LpFsStd` behind `#[cfg(feature = "std")]`

Tests to add:

- `legacy_node_kind_from_path` accepts `.texture`, `.shader`, `.output`,
  `.fixture`.
- `legacy_node_kind_from_path` rejects unknown suffixes.
- `legacy_node_config_path` appends `node.toml`.
- `load_legacy_node_config` parses a representative shader TOML.
- `discover_legacy_node_dirs` returns only legacy node directories from `/src`.
- `lpfs` trait impl can load a small in-memory node config through the generic
  loader.

Keep all test modules at the bottom of their files.

## Validate

Run from the repository root:

```bash
cargo test -p lpc-source
cargo test -p lpfs
```
