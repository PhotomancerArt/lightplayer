# Phase 02 — Def walker + shell helpers

**Dispatch:** [sub-agent: yes, model: composer-2.5-fast, parallel: -]

## Scope of phase

Implement static discovery of nested node defs and shell comparison for parent
`changed` detection.

**In scope:**

- `def_walker.rs` — enumerate `NodeInvocation` sites from parsed `NodeDef`
- `def_shell.rs` — shell view with inline bodies replaced by kind-only stubs
- Unit tests for walker paths and shell/body distinction

**Out of scope:** `NodeDefRegistry` store integration, `sync`.

## Code Organization Reminders

- Walker and shell are pure functions over `NodeDef` / `lpc-model` types.
- Tests at bottom of each file.
- Helper functions below tests in test module.

## Sub-agent Reminders

- Do **not** commit.
- Do **not** edit `lpc-engine` or `lpc-model`.
- Report deviations.

## Implementation Details

### Walker output type

```rust
pub struct InvocationSite {
    pub path: SlotPath,           // path to the NodeInvocation field
    pub invocation: NodeInvocation, // clone or ref in API — clone is fine for M2
}
```

```rust
pub fn collect_invocations(def: &NodeDef, base: &SlotPath) -> Vec<InvocationSite>;
```

**Traversal rules:**

| `NodeDef` variant | Walk |
|-------------------|------|
| `Project` | For each `(name, inv)` in `nodes.entries`: path = `base.join_field(name)` |
| `Playlist` | For each `(key, entry)` in `entries.entries`: path = `base.join_field("entries").join_key(key).join_field("node")` |
| Other | No nested invocations (empty) |

Use existing `SlotPath` APIs (`join_field`, `join_key`, or equivalent helpers
from `lpc-model`). If no join helpers exist, extend path by pushing
`SlotPathSegment` directly in this crate only — do not modify `lpc-model`.

For **`Inline` defs** registered at a path: after registering inline body,
recurse `collect_invocations(inline_def, &path)` to find nested invocations
(e.g. inline playlist with inline entries).

### Path resolution helper

```rust
pub fn resolve_node_specifier(
    containing_file: &LpPath,
    locator: &ArtifactSpecifier,
) -> Result<LpPathBuf, RegistryError>;
```

Mirror engine `resolve_path_specifier_from_dir` logic:

- `ArtifactSpecifier::Path`: absolute as-is; relative joined to containing file's
  parent directory.
- `ArtifactSpecifier::Lib`: `RegistryError` (unsupported).

Use `lpfs::LpPath` / `LpPathBuf`. Reference:
`lpc-engine/src/engine/project_loader.rs` `resolve_path_specifier_from_dir` (read
only — do not edit engine).

### Shell helpers (`def_shell.rs`)

```rust
/// Parent-facing view: inline invocation bodies replaced with kind-only stubs.
pub fn def_shell(def: &NodeDef) -> NodeDef;

/// True when full bodies differ (for leaf / inline-child entries).
pub fn body_changed(before: &NodeDef, after: &NodeDef) -> bool;

/// True when parent shells differ.
pub fn shell_changed(before: &NodeDef, after: &NodeDef) -> bool;
```

**Shell stub rule for inline invocations:**

- Replace `NodeDefRef::Inline(full)` with `NodeDefRef::Inline(Box::new(kind_stub))`
  where `kind_stub` is a minimal `NodeDef` of the same `NodeKind` (default
  variant / empty struct — match pattern used in model tests).
- Path invocations: compare locator string (`ArtifactSpecifier` display / path
  field).

**Kind in shell:** stub carries `NodeKind` — kind flip at invocation site
must make `shell_changed` return true (feeds parent `changed` in phase 04).

### Tests (this phase)

1. **Project invocation paths** — parse minimal project TOML via
   `NodeDef::from_toml_str`, assert sites at `nodes.clock`, `nodes.shader`.
2. **Playlist inline path** — use inline child TOML from
   `playlist_entry.rs` test; assert site at `entries.2.node`.
3. **Shell vs body** — two playlist defs differing only in inline shader slot
   → `body_changed` true on inline child, `shell_changed` false on playlist.
4. **Kind flip** — inline child kind `Shader` → `Clock` → `shell_changed` true
   on playlist.

Keep tests under ~20 lines each; extract TOML fixtures to helper fns at bottom
of test module.

## Validate

```bash
cargo test -p lpc-node-registry def_walker
cargo test -p lpc-node-registry def_shell
cargo clippy -p lpc-node-registry --all-targets -- -D warnings
```
