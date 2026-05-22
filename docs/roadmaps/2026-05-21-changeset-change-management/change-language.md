# ChangeSet Change Language (v1)

Canonical edit vocabulary for client-driven changes. Lives in
`lpc-node-registry/src/change/` — **serde types, not part of the slot system**.
Apply uses slot mut access + overlay tables; ops themselves are not `SlotData`.

## Top level

```text
ChangeSet { id, changes: Vec<ArtifactChange> }
```

Changes are **grouped by artifact**. Each block targets one file and lists ops
for that file only.

## Target

```rust
enum ArtifactTarget {
    Id(ArtifactId),     // committed artifact (optional; harness/wire rarely)
    Path(LpPathBuf),    // absolute project path — primary authoring form
}
```

**Implicit create:** resolving `Path(p)` get-or-creates a pending overlay entry
when `p` is not in base or overlay. No explicit `Create` op.

Overlay does not use base-store refcount rules; pending paths exist until commit
or discard.

## Ops (per artifact)

```rust
ArtifactChange {
    target: ArtifactTarget,
    ops: Vec<ArtifactOp>,
}
```

### File-level (`ArtifactOp`)

| Op | Use |
|----|-----|
| `Delete` | Remove this path on commit |
| `SetBytes(text)` | Whole-file body — GLSL, SVG, etc.; optional TOML import escape hatch |

Normal **node TOML** bodies are **not** authored with `SetBytes`. They come from
slot ops + slot codec serialize on commit.

### Slot-level (`ArtifactOp`)

Node defs are slots. All node editing is slot ops at a `SlotPath` **within** the
target artifact:

| Op | Use |
|----|-----|
| `SetSlot { path, value }` | Scalar / enum / value (includes kind, path locators, wiring) |
| `MapInsert { path, key, … }` | Map entry |
| `MapRemove { path, key }` | Map entry |
| `OptionSet { path, present }` | Option some/none (`some` → shape default) |

Examples:

- Standalone shader file: ops at `path = root()` on `/shader.toml`
- Inline child: ops at `path = entries[2].node` on `/playlist.toml`
- Wire to child file: `SetSlot` on `/project.toml` at `nodes[shader]` setting def
  path locator (not a separate “invocation op”)

Relative locators in slot values resolve against the **containing artifact path**
(same as `resolve_node_locator` today).

## Node TOML vs assets

Same `ArtifactChange` shape. Convention:

- **`.toml`** — slot ops; serialize to text on commit
- **`.glsl`, `.svg`, …** — typically `SetBytes` / `Delete`

## Creatability

Every `examples/*` project must be reachable from blank via a finite
`ChangeSet` sequence using only:

- `ArtifactChange { target: Path(...), ops: [...] }` (implicit create)
- Slot ops + `SetBytes` for assets
- No `CreateDef`; no pre-populated def blobs as the primary path

New node at artifact root: slot ops at `root()` (e.g. set kind → applies
`KindDef::default()`, then patch slots).

## Apply / commit

1. **Apply** — merge each `ArtifactChange` into path-keyed overlay; base untouched
2. **View** — `NodeDefView` / read API resolves `(path, slot_path)` over overlay ∪ base
3. **Commit** — serialize overlay TOML + assets → store/fs; registry re-derive →
   `SyncResult`
4. **Discard** — drop overlay; reads = base

Dangling path refs (wire before target file exists) are OK mid-sequence.

Unreferenced overlay paths may exist on disk after commit; registry only registers
defs reachable from root (same as filesystem reality).

## Example (add shader to project)

```text
ArtifactChange { target: Path("/shader.glsl"), ops: [ SetBytes("…") ] }

ArtifactChange {
  target: Path("/shader.toml"),
  ops: [
    SetSlot(root, kind, Shader),
    SetSlot(root.source, path, "shader.glsl"),
    …
  ],
}

ArtifactChange {
  target: Path("/project.toml"),
  ops: [ SetSlot(nodes[shader].def, path, "./shader.toml") ],
}
```

Order of blocks may vary.
