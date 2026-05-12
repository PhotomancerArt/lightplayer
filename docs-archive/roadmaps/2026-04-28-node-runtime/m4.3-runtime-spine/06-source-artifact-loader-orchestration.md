# Phase 6 — Source Artifact Loader Orchestration

sub-agent: yes
parallel: -

# Scope of phase

Add engine-side helper(s) that connect `ArtifactManager<A>` to
`lpc-source::load_artifact` without replacing the legacy loader.

Out of scope:

- Do not replace `project::legacy_loader`.
- Do not change current `ProjectRuntime::load_nodes`.
- Do not add `ProjectDomain`.
- Do not implement visual artifact unions.
- Do not broaden source parsing behavior.

# Code organization reminders

- Keep source artifact loading helpers close to the runtime artifact module.
- One concept per file if new helpers are non-trivial.
- Tests live at the bottom.
- Keep legacy and spine paths clearly named.

# Sub-agent reminders

- Do not commit.
- Do not expand into legacy cutover.
- Do not suppress warnings.
- Do not weaken tests.
- If `lpc-source::load_artifact` trait bounds do not fit this helper,
  stop and report instead of redesigning source loading.
- Report files changed, validation commands/results, and deviations.

# Implementation details

Read `00-notes.md` and `00-design.md` first.

Current source loader:

- `lpc_source::load_artifact`
- `lpc_source::ArtifactReadRoot`
- `lpc_source::SrcArtifact`
- `lpc_source::SrcArtifactSpec`

Current legacy loader:

- `lp-core/lpc-engine/src/project/legacy_loader.rs`

Add an engine-side helper in `lp-core/lpc-engine/src/artifact/`, e.g.
`source_loader.rs` if needed, that provides a small adapter:

```rust
pub fn load_source_artifact<A, R>(
    fs: &R,
    spec: &SrcArtifactSpec,
) -> Result<A, ArtifactError>
where
    A: lpc_source::SrcArtifact + serde::de::DeserializeOwned,
    R: lpc_source::ArtifactReadRoot,
```

Exact bounds may differ based on `lpc-source` APIs. Keep the helper
simple: convert `SrcArtifactSpec` to the path type expected by
`lpc_source::load_artifact`, delegate, and map errors to `ArtifactError`.

Add a convenience on `ArtifactManager<A>` if ergonomic:

```rust
manager.load_with(&artifact_ref, frame, |spec| load_source_artifact(fs, spec))
```

Do not make `ArtifactManager` itself depend on a filesystem.

Tests should use an in-memory or dummy `ArtifactReadRoot` if one exists in
the repo. If no convenient in-memory reader exists, write a tiny test-only
reader in the test module.

Test:

- a simple dummy `SrcArtifact` loads through the helper and manager.
- schema version mismatch maps to `ArtifactError`.
- legacy loader module still compiles and remains named `legacy_loader`.

# Validate

Run:

```bash
cargo +nightly fmt
cargo check -p lpc-engine -p lpc-source
cargo test -p lpc-engine artifact::
```
