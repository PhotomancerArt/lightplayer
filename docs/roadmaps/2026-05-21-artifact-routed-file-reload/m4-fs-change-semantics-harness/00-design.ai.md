# M4 Design — Fs-Change Semantics Harness

## Scope

Refine **`NodeDefRegistry`** to a simple stateful API: **apply changes → update
state → return diff**. Prove semantics in tests. No `lpc-engine` edits.

**Registry = past tense.** Engine policy documented in `engine-policy-v1.ai.md` only.

## Core API (revised from M2)

M2 split `apply_fs_changes` + `sync` across driver and store. M4 consolidates:

**Registry owns its state** (including `ArtifactStore`). Caller does not orchestrate
store bumps separately.

```rust
impl NodeDefRegistry {
    /// Bootstrap once. Registry acquires artifacts and registers defs.
    pub fn load_root(
        &mut self,
        fs: &dyn LpFs,
        root_path: &LpPath,
        frame: Revision,
        ctx: &ParseCtx<'_>,
    ) -> Result<NodeDefId, RegistryError>;

    /// Apply incoming changes, update internal state, return summary.
    pub fn sync(
        &mut self,
        fs: &dyn LpFs,
        changes: &[RegistryChange],
        frame: Revision,
        ctx: &ParseCtx<'_>,
    ) -> SyncResult;
}
```

M4: `RegistryChange::Fs(FsChange)` only.  
M5: extend enum with ChangeSet commit ops — **same `sync` entry point**.

### `sync` contract (functional)

1. Snapshot minimal **before** state needed for diff (def states, source versions).
2. **Apply** each change to internal artifact store / registry state.
3. **Re-derive** affected defs (paths in change batch + dependents).
4. **Diff** before vs after → `SyncResult`.
5. Return summary; updated state remains in registry.

No separate public `ArtifactStore`. No caller-side `apply_fs_changes`. No
harness-side source index with cross-call snapshots.

Internal bookkeeping (e.g. which artifact backs which def) is implementation
detail — not a second API surface.

## File structure

```
lp-core/lpc-node-registry/src/
├── registry/
│   ├── node_def_registry.rs    # owns ArtifactStore; load_root + sync
│   ├── sync_result.rs          # SyncResult, DefChangeDetail, SourceRevisionBump
│   ├── registry_change.rs      # RegistryChange enum (Fs in M4)
│   ├── source_deps.rs          # per-entry source dep + version (internal)
│   └── source_bridge.rs        # ShaderDef/SvgPath → M3 materialize (internal)
├── harness/                    # #[cfg(test)] — fixtures + helpers only
│   ├── fixtures.rs
│   └── bindings.rs
└── tests/
    └── fs_change_semantics.rs
```

## Types

### `RegistryChange`

```rust
pub enum RegistryChange {
    Fs(FsChange),
    // M5: ChangeSetCommit(...), AssetReplace(...), etc.
}
```

### `SyncResult`

```rust
pub struct SourceRevisionBump {
    pub def_id: NodeDefId,
    pub before: Revision,
    pub after: Revision,
}

pub enum DefChangeDetail {
    Content,
    KindChanged { from: NodeKind, to: NodeKind },
    EnteredError,
    LeftError,
}

pub struct SyncResult {
    pub def_updates: NodeDefUpdates,
    pub source_revisions: Vec<SourceRevisionBump>,
    pub change_details: Vec<(NodeDefId, DefChangeDetail)>,
}
```

`NodeDefUpdates` fields → **`Vec<NodeDefId>`** (embedded-friendly).

### Source revisions (inside `sync`, not harness)

When a fs change touches a file artifact (e.g. `/shader.glsl`):

- Registry finds defs with that source dependency (recorded on entry at load/re-derive).
- Re-materializes via M3 bridge; if version increased → push `SourceRevisionBump`.
- Def TOML unchanged → def **not** in `def_updates.changed`.

## Memory / `lp-collection`

Cold path — prefer RAM over CPU. `Vec` in `SyncResult`. Consider `DenseIdMap` for
registry entries when refactoring internals. `lp-collection` optional; add when used.

## Scenario matrix (gate)

| ID | Scenario | `sync(changes)` → |
|----|----------|-------------------|
| S1 | Leaf TOML edit | root in `def_updates.changed` |
| S2 | GLSL edit only | empty def updates; shader in `source_revisions` |
| S3 | SVG edit only | empty def updates; fixture in `source_revisions` |
| S4 | Inline child edit | child changed; parent not |
| S5a | Leaf parse error | root changed; `EnteredError` |
| S5b | Inline parse error | child changed; `EnteredError` |
| S6 | Kind change | both changed; `KindChanged` |

## Validation

```bash
cargo +nightly fmt --all
cargo test -p lpc-node-registry
cargo test -p lpc-node-registry --test fs_change_semantics
cargo clippy -p lpc-node-registry --all-targets --no-deps -- -D warnings
```

## Plan phases

| # | Phase | Dispatch |
|---|-------|----------|
| 01 | Unified sync API + SyncResult + Vec updates | composer-2.5-fast |
| 02 | Source deps + revisions inside sync | composer-2.5-fast |
| 03 | DefChangeDetail diff in sync | composer-2.5-fast |
| 04 | Scenario tests S1–S6 | composer-2.5-fast |
| 05 | M2 test migration + summary + cleanup | supervised |
