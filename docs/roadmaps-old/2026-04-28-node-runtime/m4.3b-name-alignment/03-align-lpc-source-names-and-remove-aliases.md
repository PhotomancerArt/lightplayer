# Phase 3 — Align `lpc-source` Names and Remove Aliases

sub-agent: yes
parallel: -

# Scope of phase

Make `lpc-source` exported authored/source-specific names consistently
use `Src*` where they have model/wire/view/engine siblings or are part of
the source schema:

- Remove root aliases such as `Binding`, `Shape`, `Slot`,
  `TextureSpec`, and `ValueSpec`.
- Update all call sites to the canonical `Src*` names.
- Rename `Artifact` and `ArtifactSpec` to `SrcArtifact` and
  `SrcArtifactSpec` if current call-site churn stays mechanical.

Out of scope:

- Changing TOML/schema behavior.
- Reworking source loading/migration logic.
- Renaming generic helper names such as `LoadError`, `FromTomlError`,
  `Registry`, or `Migration` unless needed by a direct source-owned
  concept rename.

# Code organization reminders

- Prefer one concept per file.
- Preserve the existing granular split of source value spec files.
- Keep serde/TOML parsing helpers in their existing helper modules.
- Do not add compatibility aliases for the removed names.

# Sub-agent reminders

- Do not commit.
- Keep this a naming/import pass.
- Do not add dependencies.
- Do not suppress warnings or weaken tests.
- If artifact renaming stops being mechanical, leave artifact names
  unchanged and report why.
- Report changed files, validation commands/results, and deviations.

# Implementation details

Read `00-notes.md` and `00-design.md` in this directory first.

In `lp-core/lpc-source/src/lib.rs`:

- Remove aliases for source names:
  - `NodeConfig = SrcNodeConfig`
  - `Binding = SrcBinding`
  - `Shape = SrcShape`
  - `Slot = SrcSlot`
  - `TextureSpec = SrcTextureSpec`
  - `ValueSpec = SrcValueSpec`
- Re-export only the canonical names.

In `lp-core/lpc-source/src/artifact/`:

- Prefer renaming:
  - `artifact.rs` -> `src_artifact.rs`
  - `artifact_spec.rs` -> `src_artifact_spec.rs`
  - `Artifact` -> `SrcArtifact`
  - `ArtifactSpec` -> `SrcArtifactSpec`
- Update `artifact/mod.rs` and all imports.
- If the artifact rename is not straightforward, stop and report rather
  than doing a partial alias-based migration.

Update call sites across:

- `lp-core/lpc-engine/src/**`
- `lp-core/lpc-wire/src/**`
- `lp-core/lpc-view/src/**`
- `lp-app/**`
- legacy crates under `lp-core/lpl-*`
- visualizer/schema helper crates if they import source names
- example/tests that compile against `lpc-source`

Search targets:

```bash
rg "\b(NodeConfig|Binding|Shape|Slot|TextureSpec|ValueSpec|Artifact|ArtifactSpec)\b" lp-core lp-app lp-visualizer
```

Use judgment with generic words like `Slot`/`Binding`; update only
`lpc-source` compatibility-name consumers, not unrelated local concepts.

# Validate

Run:

```bash
cargo +nightly fmt
cargo check -p lpc-source -p lpc-engine -p lpc-wire -p lpc-view
cargo test -p lpc-source
```
